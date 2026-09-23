//! M1 baseline: a random chord-tone melody. Later milestones replace this
//! with motif generation + beam search; it stays as the "random" baseline.

use crate::model::{chord_at, Event, Melody, Meter, Note, Style};
use crate::parser::ChordSpan;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Singable default range: C4..=G5.
pub const RANGE_LO: u8 = 60;
pub const RANGE_HI: u8 = 79;

pub fn rng(seed: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed)
}

/// One-bar pop rhythm patterns for 4/4 (durations in 16th steps, sum 16).
const POP_PATTERNS_44: &[&[u32]] = &[
    &[4, 4, 4, 4],
    &[2, 2, 4, 4, 4],
    &[4, 2, 2, 4, 4],
    &[4, 4, 2, 2, 4],
    &[2, 2, 2, 2, 4, 4],
    &[4, 4, 2, 2, 2, 2],
    &[6, 2, 4, 4],
    &[4, 6, 2, 4],
    &[2, 2, 4, 2, 2, 4],
    &[3, 3, 2, 4, 4],
    &[8, 4, 4],
    &[4, 4, 8],
    &[6, 6, 4],
    &[2, 2, 2, 2, 2, 2, 4],
    &[4, 2, 2, 2, 2, 4],
];

/// Pop phrase-ending bar patterns for 4/4: end on a long note.
const POP_ENDINGS_44: &[&[u32]] = &[&[8, 8], &[4, 4, 8], &[4, 12], &[6, 2, 8], &[2, 2, 12]];

/// Classical 4/4 patterns: even values, dotted long-short figures, and
/// pickups (a short value at the end of the bar leading into the next).
const CLASSICAL_PATTERNS_44: &[&[u32]] = &[
    &[4, 4, 4, 4],
    &[8, 4, 4],
    &[4, 4, 8],
    &[8, 8],
    &[6, 2, 4, 4],
    &[6, 2, 6, 2],
    &[4, 4, 6, 2],
    &[8, 6, 2],
    &[2, 2, 2, 2, 4, 4],
    &[4, 4, 2, 2, 2, 2],
    &[2, 2, 2, 2, 8],
    &[4, 4, 4, 2, 2],
    &[8, 4, 2, 2],
    &[12, 2, 2],
    &[4, 2, 2, 4, 4],
];

/// Classical phrase endings: a long note, or a long note plus a pickup
/// into the next phrase.
const CLASSICAL_ENDINGS_44: &[&[u32]] = &[&[8, 8], &[4, 12], &[12, 4], &[6, 2, 8], &[8, 4, 4]];

fn patterns(style: Style) -> (&'static [&'static [u32]], &'static [&'static [u32]]) {
    match style {
        Style::Classical | Style::Orchestral | Style::Brass | Style::Concerto | Style::Waltz | Style::Rapids => (CLASSICAL_PATTERNS_44, CLASSICAL_ENDINGS_44),
        Style::Pop => (POP_PATTERNS_44, POP_ENDINGS_44),
    }
}

/// Fallback for other meters: random durations, ending bars hold a long note.
fn random_rhythm(rng: &mut ChaCha8Rng, steps: u32, ending: bool) -> Vec<u32> {
    let pool: [u32; 6] = [2, 4, 4, 4, 6, 8];
    let mut out = Vec::new();
    let mut left = steps;
    let target = if ending { steps / 2 } else { 0 };
    while left > target {
        let candidates: Vec<u32> =
            pool.iter().copied().filter(|&d| d <= left - target).collect();
        let d = *candidates.choose(rng).unwrap_or(&(left - target));
        out.push(d);
        left -= d;
    }
    if left > 0 {
        out.push(left);
    }
    out
}

/// Plan the rhythm of every bar as a sentence: bars 1-2 state a motif (a, b),
/// bar 3 repeats a, bar 4 ends the phrase. The next four bars reuse the
/// motif with one bar swapped for variation. Every 4th bar and the last bar
/// end with a long note.
pub fn plan_rhythm(rng: &mut ChaCha8Rng, meter: Meter, bars: u32, style: Style) -> Vec<Vec<u32>> {
    let (pats, ends) = patterns(style);
    let spb = meter.steps_per_bar();
    let is_44 = meter.num == 4 && meter.den == 4;
    let pick = |rng: &mut ChaCha8Rng| -> Vec<u32> {
        if is_44 {
            pats.choose(rng).unwrap().to_vec()
        } else {
            random_rhythm(rng, spb, false)
        }
    };
    let pick_end = |rng: &mut ChaCha8Rng| -> Vec<u32> {
        if is_44 {
            ends.choose(rng).unwrap().to_vec()
        } else {
            random_rhythm(rng, spb, true)
        }
    };

    let a = pick(rng);
    let mut b = pick(rng);
    while b == a {
        b = pick(rng);
    }
    let variant = pick(rng);

    let mut out = Vec::with_capacity(bars as usize);
    for bar in 0..bars {
        let last = bar + 1 == bars;
        let phrase_pos = bar % 4;
        let second_phrase = (bar / 4) % 2 == 1;
        let pattern = if last {
            vec![spb]
        } else if phrase_pos == 3 {
            pick_end(rng)
        } else if phrase_pos == 1 {
            if second_phrase { variant.clone() } else { b.clone() }
        } else {
            a.clone()
        };
        out.push(pattern);
    }
    out
}

/// All chord tones within the range, as MIDI pitches.
pub fn chord_pitches(spans: &[ChordSpan], step: u32) -> Vec<u8> {
    let Some(chord) = chord_at(spans, step) else {
        return vec![];
    };
    (RANGE_LO..=RANGE_HI).filter(|&p| chord.contains_midi(p)).collect()
}

/// Pick a chord tone, preferring ones close to the previous pitch.
fn pick_pitch(rng: &mut ChaCha8Rng, choices: &[u8], prev: Option<u8>) -> u8 {
    let Some(prev) = prev else {
        return *choices.choose(rng).unwrap();
    };
    // Weight = 1 / (1 + distance in semitones / 2), so steps and thirds
    // dominate but leaps still happen. Repeating the same pitch is damped.
    let weights: Vec<f32> = choices
        .iter()
        .map(|&p| {
            let d = (p as f32 - prev as f32).abs();
            if d == 0.0 { 0.3 } else { 1.0 / (1.0 + d / 2.0) }
        })
        .collect();
    let total: f32 = weights.iter().sum();
    let mut r = rng.gen::<f32>() * total;
    for (p, w) in choices.iter().zip(&weights) {
        if r < *w {
            return *p;
        }
        r -= w;
    }
    *choices.last().unwrap()
}

pub fn random_chord_tone_melody(
    spans: &[ChordSpan],
    meter: Meter,
    bars: u32,
    seed: u64,
    style: Style,
) -> Melody {
    let mut rng = rng(seed);
    let spb = meter.steps_per_bar();
    let mut melody = Melody::default();
    let mut prev: Option<u8> = None;
    let rhythm = plan_rhythm(&mut rng, meter, bars, style);
    for bar in 0..bars {
        let mut pos = bar * spb;
        for dur in rhythm[bar as usize].iter().copied() {
            let choices = chord_pitches(spans, pos);
            if choices.is_empty() {
                melody.push(Event::Rest { start: pos, dur });
            } else {
                let pitch = pick_pitch(&mut rng, &choices, prev);
                melody.push(Event::Note(Note { pitch, start: pos, dur }));
                prev = Some(pitch);
            }
            pos += dur;
        }
    }
    melody
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_progression;

    #[test]
    fn patterns_fill_a_bar_and_motif_repeats() {
        for style in [Style::Classical, Style::Pop] {
            let (pats, ends) = patterns(style);
            for p in pats.iter().chain(ends) {
                assert_eq!(p.iter().sum::<u32>(), 16, "{style:?} {p:?}");
            }
        }
        let meter = Meter { num: 4, den: 4 };
        let plan = plan_rhythm(&mut rng(7), meter, 8, Style::Classical);
        assert_eq!(plan.len(), 8);
        assert_eq!(plan[0], plan[2]);
        assert_eq!(plan[0], plan[4]);
        assert_ne!(plan[0], plan[1]);
        assert_eq!(plan[7], vec![16]);
        assert!(plan[3].iter().any(&|&d| d >= 8));
        // Other meters still tile the bar.
        let meter = Meter { num: 3, den: 4 };
        for bar in plan_rhythm(&mut rng(7), meter, 8, Style::Pop) {
            assert_eq!(bar.iter().sum::<u32>(), 12);
        }
    }

    #[test]
    fn deterministic_and_chord_tones_only() {
        let meter = Meter { num: 4, den: 4 };
        let spans = parse_progression("C G Am F", meter, 8).unwrap();
        let a = random_chord_tone_melody(&spans, meter, 8, 42, Style::Classical);
        let b = random_chord_tone_melody(&spans, meter, 8, 42, Style::Classical);
        let c = random_chord_tone_melody(&spans, meter, 8, 43, Style::Classical);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.total_steps(), 8 * 16);
        for n in a.notes() {
            let chord = chord_at(&spans, n.start).unwrap();
            assert!(chord.contains_midi(n.pitch), "{} not in {chord}", n.pitch);
            assert!((RANGE_LO..=RANGE_HI).contains(&n.pitch));
        }
        // Events tile the grid with no gaps or overlaps.
        let mut t = 0;
        for e in &a.events {
            assert_eq!(e.start(), t);
            t = e.end();
        }
    }
}
