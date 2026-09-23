//! Motifs: a bar of material as (rhythm, diatonic contour), and the
//! transformations that turn one bar into a related bar.

use crate::model::{chord_at, Event, Meter, Note};
use crate::parser::ChordSpan;
use crate::theory::{Key, PitchClass};

/// Scale-index arithmetic: index 0 = tonic in octave 0, 7 = tonic an
/// octave up. Chromatic pitches map to the nearest lower scale degree.
pub fn scale_index(key: &Key, midi: u8) -> i32 {
    let iv = key.scale_intervals();
    let pc = key.tonic.interval_to(PitchClass::of_midi(midi)) as i32;
    let deg = iv.iter().rposition(|&i| i <= pc).unwrap_or(0) as i32;
    // Octave counted from the tonic below this pitch.
    let tonic_below = midi as i32 - pc;
    (tonic_below / 12) * 7 + deg
}

pub fn scale_pitch(key: &Key, idx: i32) -> u8 {
    let iv = key.scale_intervals();
    let oct = idx.div_euclid(7);
    let deg = idx.rem_euclid(7) as usize;
    (oct * 12 + key.tonic.0 as i32 + iv[deg]).clamp(0, 127) as u8
}

/// A bar of material relative to its first note.
#[derive(Clone, Debug, PartialEq)]
pub struct Motif {
    /// Durations in grid steps; `None` pitch = rest.
    pub rhythm: Vec<u32>,
    /// Scale-index offset from the first note, or None for a rest.
    pub degrees: Vec<Option<i32>>,
    /// Scale index of the first note (absolute).
    pub base: i32,
}

impl Motif {
    pub fn from_events(key: &Key, events: &[Event]) -> Option<Motif> {
        let first = events.iter().find_map(Event::note)?;
        let base = scale_index(key, first.pitch);
        Some(Motif {
            rhythm: events.iter().map(Event::dur).collect(),
            degrees: events
                .iter()
                .map(|e| e.note().map(|n| scale_index(key, n.pitch) - base))
                .collect(),
            base,
        })
    }

    pub fn len_steps(&self) -> u32 {
        self.rhythm.iter().sum()
    }

    /// Realize on a bar starting at `bar_start` with the first note at
    /// scale index `base`. Strong-beat non-chord tones are nudged to the
    /// nearest chord tone (max 2 semitones) so the shape fits the harmony.
    pub fn realize(
        &self,
        key: &Key,
        chords: &[ChordSpan],
        meter: Meter,
        bar_start: u32,
        base: i32,
        lo: u8,
        hi: u8,
    ) -> Vec<Event> {
        let mut out = Vec::with_capacity(self.rhythm.len());
        let mut pos = bar_start;
        for (dur, deg) in self.rhythm.iter().zip(&self.degrees) {
            match deg {
                None => out.push(Event::Rest { start: pos, dur: *dur }),
                Some(d) => {
                    let mut pitch = scale_pitch(key, base + d);
                    while pitch < lo {
                        pitch += 12;
                    }
                    while pitch > hi {
                        pitch = pitch.saturating_sub(12);
                    }
                    let strong = meter.is_strong(pos % meter.steps_per_bar());
                    if strong {
                        if let Some(chord) = chord_at(chords, pos) {
                            if !chord.contains_midi(pitch) {
                                for delta in [-1i32, 1, -2, 2] {
                                    let p = (pitch as i32 + delta).clamp(lo as i32, hi as i32) as u8;
                                    if chord.contains_midi(p) {
                                        pitch = p;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    out.push(Event::Note(Note { pitch, start: pos, dur: *dur }));
                }
            }
            pos += dur;
        }
        out
    }

    // ---- transformations -------------------------------------------------

    pub fn inverted(&self) -> Motif {
        Motif { degrees: self.degrees.iter().map(|d| d.map(|x| -x)).collect(), ..self.clone() }
    }

    pub fn retrograde_rhythm(&self) -> Motif {
        let mut rhythm = self.rhythm.clone();
        rhythm.reverse();
        Motif { rhythm, ..self.clone() }
    }

    /// Halve every duration and play the motif twice, the second time
    /// shifted by `shift` degrees (a sequence within the bar). Notes that
    /// would fall below one grid step are dropped.
    pub fn diminished(&self, shift: i32) -> Motif {
        let mut rhythm = Vec::new();
        let mut degrees = Vec::new();
        for pass in 0..2 {
            for (r, d) in self.rhythm.iter().zip(&self.degrees) {
                let half = r / 2;
                if half == 0 {
                    continue;
                }
                rhythm.push(half);
                degrees.push(d.map(|x| x + pass * shift));
            }
        }
        // Pad if the halves did not fill the bar (odd durations).
        let total: u32 = rhythm.iter().sum();
        let target = self.len_steps();
        if total < target {
            if let Some(last) = rhythm.last_mut() {
                *last += target - total;
            }
        }
        Motif { rhythm, degrees, base: self.base }
    }

    /// Double durations, keeping the first half of the motif.
    pub fn augmented(&self) -> Motif {
        let target = self.len_steps();
        let mut rhythm = Vec::new();
        let mut degrees = Vec::new();
        let mut total = 0;
        for (r, d) in self.rhythm.iter().zip(&self.degrees) {
            let dbl = (r * 2).min(target - total);
            if dbl == 0 {
                break;
            }
            rhythm.push(dbl);
            degrees.push(*d);
            total += dbl;
        }
        Motif { rhythm, degrees, base: self.base }
    }

    /// Split every note of at least a quarter into two: the note and a
    /// neighbor/passing tone toward the next note.
    pub fn ornamented(&self) -> Motif {
        let mut rhythm = Vec::new();
        let mut degrees = Vec::new();
        let n = self.rhythm.len();
        for i in 0..n {
            let (r, d) = (self.rhythm[i], self.degrees[i]);
            let next = self.degrees.get(i + 1).copied().flatten();
            match (d, next) {
                (Some(d), Some(nx)) if r >= 4 && i + 1 < n => {
                    let dir = if nx > d { 1 } else if nx < d { -1 } else { 1 };
                    rhythm.push(r / 2);
                    degrees.push(Some(d));
                    rhythm.push(r - r / 2);
                    degrees.push(Some(d + dir));
                }
                _ => {
                    rhythm.push(r);
                    degrees.push(d);
                }
            }
        }
        Motif { rhythm, degrees, base: self.base }
    }

    /// Drop the last note and extend the one before it.
    pub fn truncated(&self) -> Motif {
        if self.rhythm.len() < 2 {
            return self.clone();
        }
        let mut m = self.clone();
        let last = m.rhythm.pop().unwrap();
        m.degrees.pop();
        *m.rhythm.last_mut().unwrap() += last;
        m
    }

    /// Split the last note if it is long enough, adding a step toward the
    /// tonic direction (`dir` = +1/-1).
    pub fn extended(&self, dir: i32) -> Motif {
        let mut m = self.clone();
        let Some(last) = m.rhythm.last().copied() else { return m };
        if last < 4 {
            return m;
        }
        let Some(Some(d)) = m.degrees.last().copied() else { return m };
        *m.rhythm.last_mut().unwrap() = last / 2;
        m.rhythm.push(last - last / 2);
        m.degrees.push(Some(d + dir));
        m
    }
}

// ---- similarity ---------------------------------------------------------

/// Direction signs of successive intervals (+1, 0, -1), rests skipped.
pub fn contour(events: &[Event]) -> Vec<i8> {
    let pitches: Vec<i32> = events.iter().filter_map(|e| e.note().map(|n| n.pitch as i32)).collect();
    pitches.windows(2).map(|w| (w[1] - w[0]).signum() as i8).collect()
}

pub fn rhythm_of(events: &[Event]) -> Vec<u32> {
    events.iter().map(Event::dur).collect()
}

pub fn pitches_of(events: &[Event]) -> Vec<u8> {
    events.iter().filter_map(|e| e.note().map(|n| n.pitch)).collect()
}

fn seq_match<T: PartialEq>(a: &[T], b: &[T]) -> f32 {
    let n = a.len().max(b.len());
    if n == 0 {
        return 1.0;
    }
    let same = a.iter().zip(b).filter(|(x, y)| x == y).count();
    same as f32 / n as f32
}

/// 0..1: half rhythm agreement, half contour agreement.
pub fn similarity(a: &[Event], b: &[Event]) -> f32 {
    0.5 * seq_match(&rhythm_of(a), &rhythm_of(b)) + 0.5 * seq_match(&contour(a), &contour(b))
}

/// Same rhythm and the same interval sequence up to diatonic adjustment
/// (each interval may differ by one semitone but keeps its direction),
/// i.e. a chromatic or diatonic transposition, including exact.
pub fn is_transposition(a: &[Event], b: &[Event]) -> bool {
    let (pa, pb) = (pitches_of(a), pitches_of(b));
    if pa.len() != pb.len() || pa.is_empty() || rhythm_of(a) != rhythm_of(b) {
        return false;
    }
    let ia: Vec<i32> = pa.windows(2).map(|w| w[1] as i32 - w[0] as i32).collect();
    let ib: Vec<i32> = pb.windows(2).map(|w| w[1] as i32 - w[0] as i32).collect();
    ia.iter().zip(&ib).all(|(x, y)| (x - y).abs() <= 1 && x.signum() == y.signum())
}

pub fn is_exact(a: &[Event], b: &[Event]) -> bool {
    rhythm_of(a) == rhythm_of(b) && pitches_of(a) == pitches_of(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_key, parse_progression};

    fn ev(notes: &[(u8, u32)]) -> Vec<Event> {
        let mut t = 0;
        notes
            .iter()
            .map(|&(p, d)| {
                let e = if p == 0 { Event::Rest { start: t, dur: d } } else { Event::Note(Note { pitch: p, start: t, dur: d }) };
                t += d;
                e
            })
            .collect()
    }

    #[test]
    fn scale_index_roundtrip() {
        let key = parse_key("A:minor").unwrap();
        for p in 57..=84u8 {
            let i = scale_index(&key, p);
            let back = scale_pitch(&key, i);
            assert!(back <= p && p - back <= 1, "{p} -> {i} -> {back}");
        }
        assert_eq!(scale_pitch(&key, scale_index(&key, 69) + 7), 81); // A4 + octave
    }

    #[test]
    fn transformations_keep_bar_length() {
        let key = parse_key("C").unwrap();
        let m = Motif::from_events(&key, &ev(&[(60, 4), (62, 4), (64, 2), (65, 2), (67, 4)])).unwrap();
        for t in [m.inverted(), m.retrograde_rhythm(), m.diminished(1), m.augmented(), m.ornamented(), m.truncated(), m.extended(-1)] {
            assert_eq!(t.len_steps(), 16, "{t:?}");
            assert_eq!(t.rhythm.len(), t.degrees.len());
        }
        assert_eq!(m.inverted().degrees, vec![Some(0), Some(-1), Some(-2), Some(-3), Some(-4)]);
        assert_eq!(m.diminished(1).rhythm.len(), 10);
    }

    #[test]
    fn realize_transposes_and_fits_chord() {
        let key = parse_key("C").unwrap();
        let meter = Meter { num: 4, den: 4 };
        let chords = parse_progression("C G", meter, 2).unwrap();
        let m = Motif::from_events(&key, &ev(&[(60, 8), (64, 8)])).unwrap(); // C E
        // Same base on bar 2 (G chord): C on the downbeat is not in G, nudged to B or D.
        let r = m.realize(&key, &chords, meter, 16, m.base, 55, 84);
        let p = pitches_of(&r);
        assert!(p[0] == 59 || p[0] == 62, "{p:?}");
        // One degree up: D F -> over G, D stays, F on beat 3 is nudged to G.
        let r = m.realize(&key, &chords, meter, 16, m.base + 1, 55, 84);
        assert_eq!(pitches_of(&r), vec![62, 67]);
    }

    #[test]
    fn similarity_measures() {
        let a = ev(&[(60, 4), (62, 4), (64, 8)]);
        let b = ev(&[(67, 4), (69, 4), (71, 8)]);
        let c = ev(&[(60, 2), (59, 2), (60, 12)]);
        assert_eq!(similarity(&a, &b), 1.0);
        assert!(is_transposition(&a, &b));
        assert!(!is_exact(&a, &b));
        assert!(is_exact(&a, &a));
        assert!(similarity(&a, &c) < 0.5);
    }
}
