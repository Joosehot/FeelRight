//! Rule 7: at a chord change, prefer the 3rd or 7th of the new chord on
//! its first strong beat.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::PitchClass;

pub struct GuideTones;

impl Rule for GuideTones {
    fn name(&self) -> &'static str {
        "guide_tones"
    }
    fn params(&self) -> &'static [&'static str] {
        &["other_chord_tone"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        let mut prev_chord = None;
        for span in ctx.chords {
            let chord = &span.chord;
            let changed = prev_chord.map(|p| p != chord).unwrap_or(true);
            prev_chord = Some(chord);
            if !changed {
                continue;
            }
            // First note that starts on a strong beat inside this span.
            let Some(note) = notes.iter().find(|nt| {
                nt.start >= span.start && nt.start < span.start + span.len && ctx.is_strong(nt.start)
            }) else {
                continue;
            };
            n += 1;
            let pc = PitchClass::of_midi(note.pitch);
            let s = if pc == chord.third() || Some(pc) == chord.seventh() {
                1.0
            } else if chord.contains(pc) {
                cfg.param("other_chord_tone")
            } else {
                0.0
            };
            if s < 1.0 {
                res.details.push(Detail {
                    bar: ctx.bar_of(note.start),
                    score: s - 1.0,
                    text: format!("{pc} on the change to {chord} is not a guide tone"),
                });
            }
            sum += s;
        }
        res.score = if n == 0 { 0.0 } else { (sum / n as f32) * 2.0 - 1.0 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_thirds_on_changes() {
        let f = fixture("C G", "C", 2, &[(E4, 16), (B4, 16)]);
        let r = GuideTones.score(&f.ctx(), f.rule_cfg("guide_tones"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_with_roots_only() {
        let f = fixture("C G", "C", 2, &[(C4, 16), (G4, 16)]);
        let r = GuideTones.score(&f.ctx(), f.rule_cfg("guide_tones"));
        assert!(r.score < 0.5, "{}", r.score);
        assert_eq!(r.details.len(), 2);
    }
}
