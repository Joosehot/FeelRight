//! M3/M4: bar-by-bar beam search.
//!
//! Every bar has a role from the form plan. Idea/Free/Cadence bars are
//! sampled fresh (rhythm from the sentence plan, pitches from a seeded
//! sampler). Repeat and Fragment bars are built mostly by transforming the
//! referenced bar of the same beam entry: transposition to fit the new
//! chord, inversion, retrograde rhythm, ornamentation, diminution,
//! truncation and extension. Every candidate prefix is scored with the full
//! rule set; hard violations are discarded; the top `beam` survive.

use crate::config::Config;
use crate::form::{Form, Role};
use crate::generate::{plan_rhythm, rng, RANGE_HI, RANGE_LO};
use crate::model::{chord_at, Event, Melody, Meter, Note, Style};
use crate::motif::Motif;
use crate::parser::ChordSpan;
use crate::rules::{evaluate, Context, Evaluation, Rule};
use crate::theory::{Key, PitchClass};
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use std::collections::HashSet;

struct Beam {
    melody: Melody,
    score: f32,
}

pub struct SearchInput<'a> {
    pub chords: &'a [ChordSpan],
    pub key: Key,
    pub meter: Meter,
    pub bars: u32,
    pub tension: &'a [f32],
    pub style: Style,
    pub seed: u64,
    pub form: &'a Form,
    pub ends_open: bool,
}

/// Pitch choices for one slot: chord tones always; diatonic scale tones on
/// weak positions (passing/neighbor material). Restricted to `max_interval`
/// around the previous pitch.
fn slot_choices(input: &SearchInput, step: u32, prev: Option<u8>, max_interval: i32) -> Vec<(u8, f32)> {
    let Some(chord) = chord_at(input.chords, step) else {
        return vec![];
    };
    let strong = input.meter.is_strong(step % input.meter.steps_per_bar());
    let scale = input.key.scale();
    let mut out = Vec::new();
    for p in RANGE_LO..=RANGE_HI {
        let pc = PitchClass::of_midi(p);
        let in_chord = chord.contains(pc);
        let in_scale = scale.contains(&pc) || pc == input.key.leading_tone();
        if !in_chord && !(in_scale && !strong) {
            continue;
        }
        let dist = prev.map(|q| (p as i32 - q as i32).abs()).unwrap_or(0);
        if dist > max_interval {
            continue;
        }
        let mut w = 1.0 / (1.0 + dist as f32 / 2.0);
        if in_chord {
            w *= 1.5;
        }
        if dist == 0 {
            w *= 0.4;
        }
        out.push((p, w));
    }
    out
}

fn weighted_pick(rng: &mut ChaCha8Rng, choices: &[(u8, f32)]) -> u8 {
    let total: f32 = choices.iter().map(|c| c.1).sum();
    let mut r = rng.gen::<f32>() * total;
    for (p, w) in choices {
        if r < *w {
            return *p;
        }
        r -= w;
    }
    choices.last().unwrap().0
}

/// Sample one bar of pitches for the given rhythm.
fn sample_bar(
    rng: &mut ChaCha8Rng,
    input: &SearchInput,
    bar: u32,
    rhythm: &[u32],
    mut prev: Option<u8>,
    max_interval: i32,
) -> Vec<Event> {
    let spb = input.meter.steps_per_bar();
    let mut pos = bar * spb;
    let mut events = Vec::with_capacity(rhythm.len());
    for &dur in rhythm {
        let choices = slot_choices(input, pos, prev, max_interval);
        if choices.is_empty() {
            events.push(Event::Rest { start: pos, dur });
        } else {
            let pitch = weighted_pick(rng, &choices);
            events.push(Event::Note(Note { pitch, start: pos, dur }));
            prev = Some(pitch);
        }
        pos += dur;
    }
    events
}

/// Deterministic transformations of a source bar, realized on `bar`.
fn transformed_candidates(input: &SearchInput, src: &[Event], bar: u32, fragment: bool) -> Vec<Vec<Event>> {
    let Some(m) = Motif::from_events(&input.key, src) else {
        return vec![];
    };
    let spb = input.meter.steps_per_bar();
    let start = bar * spb;
    let key = &input.key;

    // Base scale indices to try: same, and shifted by up to a third either
    // way (transposition following the harmony).
    let bases: Vec<i32> = (-2..=2).map(|d| m.base + d).collect();

    let mut shapes: Vec<Motif> = if fragment {
        vec![
            m.diminished(1),
            m.diminished(-1),
            m.diminished(0),
            m.truncated(),
            m.clone(),
            m.inverted(),
        ]
    } else {
        vec![
            m.clone(),
            m.inverted(),
            m.retrograde_rhythm(),
            m.ornamented(),
            m.truncated(),
            m.extended(-1),
            m.extended(1),
            m.augmented(),
        ]
    };
    shapes.dedup();

    let mut out = Vec::new();
    for shape in &shapes {
        for &base in &bases {
            let ev = shape.realize(key, input.chords, input.meter, start, base, RANGE_LO, RANGE_HI);
            if ev.iter().map(Event::dur).sum::<u32>() == spb {
                out.push(ev);
            }
        }
    }
    out
}

fn score_prefix(input: &SearchInput, melody: &Melody, complete: bool, cfg: &Config, rules: &[Box<dyn Rule>]) -> Evaluation {
    let ctx = Context {
        melody,
        chords: input.chords,
        key: input.key,
        meter: input.meter,
        bars: input.bars,
        tension: input.tension,
        complete,
        form: input.form,
        ends_open: input.ends_open,
    };
    evaluate(&ctx, cfg, rules)
}

pub fn beam_search(input: &SearchInput, cfg: &Config, rules: &[Box<dyn Rule>]) -> (Melody, Evaluation) {
    let mut rng = rng(input.seed);
    let rhythm = plan_rhythm(&mut rng, input.meter, input.bars, input.style);
    let width = cfg.search.beam.max(1);
    let k = cfg.search.candidates_per_bar.max(1);
    let max_iv = cfg.search.max_interval;

    let mut beams = vec![Beam { melody: Melody::default(), score: 0.0 }];
    for bar in 0..input.bars {
        let complete = bar + 1 == input.bars;
        let role = input.form.role(bar);
        let mut next: Vec<Beam> = Vec::new();
        let mut seen: HashSet<Vec<(u8, u32)>> = HashSet::new();
        for b in &beams {
            let prev = b.melody.notes().last().map(|n| n.pitch);
            let mut candidates: Vec<Vec<Event>> = Vec::new();
            let fresh = match role {
                Role::Repeat { of } | Role::Fragment { of } => {
                    let src = b.melody.bar_events(of, input.meter);
                    candidates.extend(transformed_candidates(input, &src, bar, matches!(role, Role::Fragment { .. })));
                    k / 4
                }
                _ => k,
            };
            for _ in 0..fresh.max(1) {
                candidates.push(sample_bar(&mut rng, input, bar, &rhythm[bar as usize], prev, max_iv));
            }
            for events in candidates {
                let mut m = b.melody.clone();
                m.events.extend(events);
                let sig: Vec<(u8, u32)> = m.notes().map(|n| (n.pitch, n.start)).collect();
                if !seen.insert(sig) {
                    continue;
                }
                let eval = score_prefix(input, &m, complete, cfg, rules);
                if eval.hard_violation {
                    continue;
                }
                next.push(Beam { melody: m, score: eval.total });
            }
        }
        if next.is_empty() {
            break;
        }
        next.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        next.truncate(width);
        beams = next;
    }
    let best = beams.into_iter().next().expect("at least one beam");
    let eval = score_prefix(input, &best.melody, true, cfg, rules);
    (best.melody, eval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Template;
    use crate::generate::random_chord_tone_melody;
    use crate::parser::{parse_key, parse_progression};
    use crate::rules::all_rules;

    fn setup() -> (Vec<ChordSpan>, Key, Meter, Form) {
        let meter = Meter { num: 4, den: 4 };
        let chords = parse_progression("Am Dm E7 Am | F Dm E7 Am", meter, 8).unwrap();
        (chords, parse_key("A:minor").unwrap(), meter, Form::plan(Template::Auto, 8))
    }

    #[test]
    fn beam_is_deterministic_and_complete() {
        let (chords, key, meter, form) = setup();
        let cfg = Config::default_config();
        let rules = all_rules();
        let input = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed: 3, form: &form, ends_open: false };
        let (a, ea) = beam_search(&input, &cfg, &rules);
        let (b, _) = beam_search(&input, &cfg, &rules);
        assert_eq!(a, b);
        assert_eq!(a.total_steps(), 8 * 16);
        assert!(!ea.hard_violation);
    }

    #[test]
    fn beam_beats_random_baseline() {
        let (chords, key, meter, form) = setup();
        let cfg = Config::default_config();
        let rules = all_rules();
        for seed in 1..=3 {
            let input = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed, form: &form, ends_open: false };
            let (_, beam_eval) = beam_search(&input, &cfg, &rules);
            let random = random_chord_tone_melody(&chords, meter, 8, seed, Style::Classical);
            let rand_eval = score_prefix(&input, &random, true, &cfg, &rules);
            assert!(beam_eval.total > rand_eval.total + 3.0, "seed {seed}: beam {} vs random {}", beam_eval.total, rand_eval.total);
        }
    }

    #[test]
    fn transformations_fill_the_bar() {
        let (chords, key, meter, form) = setup();
        let input = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed: 1, form: &form, ends_open: false };
        let src = vec![
            Event::Note(Note { pitch: 69, start: 0, dur: 4 }),
            Event::Note(Note { pitch: 71, start: 4, dur: 4 }),
            Event::Note(Note { pitch: 72, start: 8, dur: 8 }),
        ];
        let c = transformed_candidates(&input, &src, 2, false);
        assert!(c.len() >= 20);
        for ev in &c {
            assert_eq!(ev[0].start(), 32);
            assert_eq!(ev.iter().map(Event::dur).sum::<u32>(), 16);
        }
    }
}
