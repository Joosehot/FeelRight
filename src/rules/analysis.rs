//! Shared melodic analysis: non-chord-tone classification and per-note
//! context, used by several harmony and expectation rules.

use super::Context;
use crate::model::Note;
use crate::theory::Chord;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nct {
    ChordTone,
    Passing,
    Neighbor,
    Appoggiatura,
    Suspension,
    Anticipation,
    Escape,
    Unclassified,
}

impl Nct {
    pub fn label(self) -> &'static str {
        match self {
            Nct::ChordTone => "chord tone",
            Nct::Passing => "passing tone",
            Nct::Neighbor => "neighbor tone",
            Nct::Appoggiatura => "appoggiatura",
            Nct::Suspension => "suspension",
            Nct::Anticipation => "anticipation",
            Nct::Escape => "escape tone",
            Nct::Unclassified => "unclassified non-chord tone",
        }
    }
}

#[derive(Clone, Debug)]
pub struct NoteInfo<'a> {
    pub note: &'a Note,
    pub chord: Option<&'a Chord>,
    pub nct: Nct,
    pub strong: bool,
    pub strength: f32,
    pub bar: u32,
    /// Interval from the previous note (semitones), if any.
    pub prev_iv: Option<i32>,
    /// Interval to the next note, if any.
    pub next_iv: Option<i32>,
    /// The chord changed between the previous note and this one.
    pub chord_changed: bool,
    /// The chord changes between this note and the next.
    pub chord_changes_next: bool,
}

fn is_step(iv: i32) -> bool {
    matches!(iv.abs(), 1 | 2)
}
fn is_leap(iv: i32) -> bool {
    iv.abs() > 2
}

pub fn analyze<'a>(ctx: &Context<'a>) -> Vec<NoteInfo<'a>> {
    let notes = ctx.notes();
    let n = notes.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let note = notes[i];
        let chord = ctx.chord_at(note.start);
        let prev = if i > 0 { Some(notes[i - 1]) } else { None };
        let next = notes.get(i + 1).copied();
        let prev_iv = prev.map(|p| note.pitch as i32 - p.pitch as i32);
        let next_iv = next.map(|q| q.pitch as i32 - note.pitch as i32);
        let prev_chord = prev.and_then(|p| ctx.chord_at(p.start));
        let next_chord = next.and_then(|q| ctx.chord_at(q.start));
        let chord_changed = prev.is_some() && prev_chord != chord;
        let chord_changes_next = next.is_some() && next_chord != chord;
        let strong = ctx.is_strong(note.start);
        let in_chord = chord.map(|c| c.contains_midi(note.pitch)).unwrap_or(true);

        let nct = if in_chord {
            Nct::ChordTone
        } else {
            match (prev_iv, next_iv) {
                (Some(a), Some(b)) if a == 0 => {
                    // Held from the previous chord.
                    let was_chord_tone = prev
                        .zip(prev_chord)
                        .map(|(p, c)| c.contains_midi(p.pitch))
                        .unwrap_or(false);
                    if chord_changed && was_chord_tone && is_step(b) {
                        Nct::Suspension
                    } else {
                        Nct::Unclassified
                    }
                }
                (Some(a), Some(b)) if is_step(a) && is_step(b) => {
                    if a.signum() == b.signum() { Nct::Passing } else { Nct::Neighbor }
                }
                (Some(a), Some(b)) if is_leap(a) && is_step(b) => {
                    if strong { Nct::Appoggiatura } else { Nct::Neighbor }
                }
                (Some(a), Some(b)) if is_step(a) && is_leap(b) && a.signum() != b.signum() => Nct::Escape,
                (_, Some(b)) if b == 0 && chord_changes_next
                    && next_chord.map(|c| c.contains_midi(note.pitch)).unwrap_or(false) => Nct::Anticipation,
                (None, Some(b)) if is_step(b) => Nct::Neighbor,
                _ => Nct::Unclassified,
            }
        };
        out.push(NoteInfo {
            note,
            chord,
            nct,
            strong,
            strength: ctx.strength(note.start),
            bar: ctx.bar_of(note.start),
            prev_iv,
            next_iv,
            chord_changed,
            chord_changes_next,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    fn kinds(f: &Fixture) -> Vec<Nct> {
        analyze(&f.ctx()).into_iter().map(|i| i.nct).collect()
    }

    #[test]
    fn classifies_passing_neighbor_appoggiatura() {
        // C D E | E F E | C A(leap) G
        let f = fixture("C", "C", 2, &[(C4, 4), (D4, 4), (E4, 4), (F4, 2), (E4, 2), (C4, 8), (A4, 4), (G4, 4)]);
        let k = kinds(&f);
        assert_eq!(k[1], Nct::Passing);
        assert_eq!(k[3], Nct::Neighbor);
        assert_eq!(k[6], Nct::Appoggiatura); // A on beat 3 of bar 2 (strong), leap in, step out
    }

    #[test]
    fn classifies_suspension_and_anticipation() {
        // Over C then G: E held into G bar then steps down to D = suspension.
        let f = fixture("C G", "C", 2, &[(E4, 8), (E4, 8), (E4, 4), (D4, 12)]);
        let k = kinds(&f);
        assert_eq!(k[2], Nct::Suspension);
        // Over C then G: D sounded before the G chord, repeated = anticipation.
        let f = fixture("C G", "C", 2, &[(C4, 8), (E4, 6), (D4, 2), (D4, 16)]);
        let k = kinds(&f);
        assert_eq!(k[2], Nct::Anticipation);
    }

    #[test]
    fn leap_in_leap_out_is_unclassified() {
        let f = fixture("C", "C", 1, &[(C4, 4), (F4, 4), (C5, 4), (E4, 4)]);
        let k = kinds(&f);
        assert_eq!(k[1], Nct::Unclassified);
    }
}
