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
use rand::seq::SliceRandom;
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
    /// Opening motif borrowed from another section (with the pitch its
    /// first note had there), used to seed bar 1 with transformations.
    pub theme: Option<&'a Theme>,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub motif: Motif,
    pub first_pitch: u8,
}

impl Theme {
    pub fn from_melody(key: &Key, melody: &Melody, meter: Meter) -> Option<Theme> {
        let bar0 = melody.bar_events(0, meter);
        let motif = Motif::from_events(key, &bar0)?;
        let first_pitch = bar0.iter().find_map(Event::note)?.pitch;
        Some(Theme { motif, first_pitch })
    }
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
        } else if chord.tones.iter().any(|t| matches!(t.interval_to(pc), 1 | 11)) {
            // A semitone against a chord tone: only as a quick passing note.
            w *= 0.3;
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
    motif_candidates(input, &m, m.base, bar, if fragment { Shapes::Fragment } else { Shapes::Repeat })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shapes {
    Repeat,
    Fragment,
    /// A borrowed theme keeps its rhythm and contour; only ornaments,
    /// extensions and truncation are allowed.
    Theme,
}

/// Transformations of a borrowed theme, realized in this section's key
/// around the pitch the theme started on.
fn theme_candidates(input: &SearchInput, theme: &Theme, bar: u32) -> Vec<Vec<Event>> {
    let center = crate::motif::scale_index(&input.key, theme.first_pitch);
    motif_candidates(input, &theme.motif, center, bar, Shapes::Theme)
}

fn motif_candidates(input: &SearchInput, m: &Motif, center: i32, bar: u32, kind: Shapes) -> Vec<Vec<Event>> {
    let spb = input.meter.steps_per_bar();
    let start = bar * spb;
    let key = &input.key;

    // Base scale indices to try: same, and shifted by up to a third either
    // way (transposition following the harmony).
    let bases: Vec<i32> = (-2..=2).map(|d| center + d).collect();

    let mut shapes: Vec<Motif> = if kind == Shapes::Theme {
        vec![m.clone(), m.ornamented(), m.extended(-1), m.extended(1), m.truncated()]
    } else if kind == Shapes::Fragment {
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
                Role::Idea if bar == 0 && input.theme.is_some() => {
                    candidates.extend(theme_candidates(input, input.theme.unwrap(), bar));
                    0
                }
                _ => k,
            };
            for _ in 0..fresh {
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
        let input = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed: 3, form: &form, ends_open: false, theme: None };
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
            let input = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed, form: &form, ends_open: false, theme: None };
            let (_, beam_eval) = beam_search(&input, &cfg, &rules);
            let random = random_chord_tone_melody(&chords, meter, 8, seed, Style::Classical);
            let rand_eval = score_prefix(&input, &random, true, &cfg, &rules);
            assert!(beam_eval.total > rand_eval.total + 3.0, "seed {seed}: beam {} vs random {}", beam_eval.total, rand_eval.total);
        }
    }

    #[test]
    fn borrowed_theme_shapes_the_opening() {
        let (chords, key, meter, form) = setup();
        let cfg = Config::default_config();
        let rules = all_rules();
        let base = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed: 3, form: &form, ends_open: false, theme: None };
        let (src, _) = beam_search(&base, &cfg, &rules);
        let theme = Theme::from_melody(&key, &src, meter).unwrap();
        let with = SearchInput { seed: 99, theme: Some(&theme), ..base };
        let (m, _) = beam_search(&with, &cfg, &rules);
        let sim = crate::motif::similarity(&src.bar_events(0, meter), &m.bar_events(0, meter));
        assert!(sim >= 0.5, "similarity {sim}");
    }

    #[test]
    fn transformations_fill_the_bar() {
        let (chords, key, meter, form) = setup();
        let input = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed: 1, form: &form, ends_open: false, theme: None };
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

// ---- iterative refinement ----------------------------------------------

/// One accepted change during refinement.
#[derive(Clone, Debug)]
pub struct RefineStep {
    pub round: u32,
    pub bar: u32,
    pub what: &'static str,
    pub before: f32,
    pub after: f32,
}

/// Replace bar `bar` of `melody` with `events` (which must already be
/// positioned at that bar).
fn splice_bar(melody: &Melody, meter: Meter, bar: u32, events: Vec<Event>) -> Melody {
    let spb = meter.steps_per_bar();
    let (a, b) = (bar * spb, (bar + 1) * spb);
    let mut out = Melody::default();
    for e in &melody.events {
        if e.start() < a {
            out.push(*e);
        }
    }
    out.events.extend(events);
    for e in &melody.events {
        if e.start() >= b {
            out.push(*e);
        }
    }
    out
}

/// Hill-climb on a finished melody: each round proposes changes to one
/// bar (fresh samples, transformations of another bar, or a single
/// scale-step nudge) and keeps the best proposal only if the full score
/// improves. Stops after `patience` rounds without improvement or after
/// `max_rounds`. Deterministic for a given seed.
pub fn refine(
    input: &SearchInput,
    cfg: &Config,
    rules: &[Box<dyn Rule>],
    start: Melody,
    max_rounds: u32,
    patience: u32,
) -> (Melody, Evaluation, Vec<RefineStep>) {
    let mut rng = rng(input.seed.wrapping_mul(7919).wrapping_add(17));
    let mut current = start;
    let mut best_eval = score_prefix(input, &current, true, cfg, rules);
    let mut log = Vec::new();
    let mut idle = 0;
    let k = cfg.search.candidates_per_bar.max(4);
    let max_iv = cfg.search.max_interval;
    let rhythm = plan_rhythm(&mut rng, input.meter, input.bars, input.style);

    for round in 1..=max_rounds {
        if idle >= patience {
            break;
        }
        let bar = rng.gen_range(0..input.bars);
        let prev = current
            .notes()
            .filter(|n| n.start < bar * input.meter.steps_per_bar())
            .last()
            .map(|n| n.pitch);
        let mut proposals: Vec<(&'static str, Vec<Event>)> = Vec::new();

        // Fresh material on this bar's current rhythm and on a library rhythm.
        let cur_rhythm: Vec<u32> = current.bar_events(bar, input.meter).iter().map(Event::dur).collect();
        for _ in 0..k / 2 {
            proposals.push(("resampled", sample_bar(&mut rng, input, bar, &cur_rhythm, prev, max_iv)));
        }
        for _ in 0..k / 4 {
            proposals.push(("new rhythm", sample_bar(&mut rng, input, bar, &rhythm[bar as usize], prev, max_iv)));
        }
        // Transformations of another bar.
        if input.bars > 1 {
            let mut src_bar = rng.gen_range(0..input.bars);
            if src_bar == bar {
                src_bar = (bar + 1) % input.bars;
            }
            let src = current.bar_events(src_bar, input.meter);
            for ev in transformed_candidates(input, &src, bar, rng.gen::<bool>()) {
                proposals.push(("transformed", ev));
            }
        }
        // Nudge one note by a scale step.
        let mut nudged = current.bar_events(bar, input.meter);
        let note_slots: Vec<usize> = nudged.iter().enumerate().filter(|(_, e)| e.note().is_some()).map(|(i, _)| i).collect();
        if let Some(&i) = note_slots.choose(&mut rng) {
            if let Event::Note(n) = &mut nudged[i] {
                let idx = crate::motif::scale_index(&input.key, n.pitch);
                let dir = if rng.gen::<bool>() { 1 } else { -1 };
                n.pitch = crate::motif::scale_pitch(&input.key, idx + dir).clamp(RANGE_LO, RANGE_HI);
            }
            proposals.push(("nudged", nudged));
        }

        let mut best_local: Option<(&'static str, Melody, Evaluation)> = None;
        for (what, ev) in proposals {
            let m = splice_bar(&current, input.meter, bar, ev);
            let e = score_prefix(input, &m, true, cfg, rules);
            if e.hard_violation {
                continue;
            }
            if best_local.as_ref().map(|b| e.total > b.2.total).unwrap_or(true) {
                best_local = Some((what, m, e));
            }
        }
        match best_local {
            Some((what, m, e)) if e.total > best_eval.total + 1e-3 => {
                log.push(RefineStep { round, bar, what, before: best_eval.total, after: e.total });
                current = m;
                best_eval = e;
                idle = 0;
            }
            _ => idle += 1,
        }
    }
    (current, best_eval, log)
}

#[cfg(test)]
mod refine_tests {
    use super::*;
    use crate::form::Template;
    use crate::parser::{parse_key, parse_progression};
    use crate::rules::all_rules;

    #[test]
    fn refine_never_lowers_the_score_and_is_deterministic() {
        let meter = Meter { num: 4, den: 4 };
        let chords = parse_progression("Am Dm E7 Am | F Dm E7 Am", meter, 8).unwrap();
        let key = parse_key("A:minor").unwrap();
        let form = Form::plan(Template::Auto, 8);
        let cfg = Config::default_config();
        let rules = all_rules();
        let input = SearchInput { chords: &chords, key, meter, bars: 8, tension: &[], style: Style::Classical, seed: 5, form: &form, ends_open: false, theme: None };
        let (m0, e0) = beam_search(&input, &cfg, &rules);
        let (m1, e1, log) = refine(&input, &cfg, &rules, m0.clone(), 20, 20);
        let (m2, _, _) = refine(&input, &cfg, &rules, m0, 20, 20);
        assert!(e1.total >= e0.total);
        assert_eq!(m1, m2);
        assert_eq!(m1.total_steps(), 8 * 16);
        for s in &log {
            assert!(s.after > s.before);
        }
    }
}
