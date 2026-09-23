//! Rule 1: notes that start on strong beats should be chord tones.
//! Breakable: a non-chord tone on a strong beat is tension.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct ChordTonesOnStrongBeats;

impl Rule for ChordTonesOnStrongBeats {
    fn name(&self) -> &'static str {
        "chord_tones_on_strong_beats"
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut sum = 0.0;
        let mut weight_sum = 0.0;
        let mut res = RuleResult::default();
        for n in ctx.notes() {
            if !ctx.is_strong(n.start) {
                continue;
            }
            let Some(chord) = ctx.chord_at(n.start) else { continue };
            let w = ctx.strength(n.start);
            weight_sum += w;
            if chord.contains_midi(n.pitch) {
                sum += w;
            } else {
                sum -= w;
                res.broken = true;
                res.tension += cfg.tension * w;
                res.details.push(Detail {
                    bar: ctx.bar_of(n.start),
                    score: -w,
                    text: format!(
                        "non-chord tone {} on strong beat over {chord}",
                        crate::theory::PitchClass::of_midi(n.pitch)
                    ),
                });
            }
        }
        res.score = if weight_sum > 0.0 { sum / weight_sum } else { 0.0 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_when_strong_beats_are_chord_tones() {
        // C major bar: C on 1, D passing on "and", E on 2, G on 3, A on 4.
        let f = fixture("C", "C", 1, &[(C4, 2), (D4, 2), (E4, 4), (G4, 4), (A4, 4)]);
        let r = ChordTonesOnStrongBeats.score(&f.ctx(), f.rule_cfg("chord_tones_on_strong_beats"));
        assert_eq!(r.score, 1.0);
        assert!(!r.broken);
    }

    #[test]
    fn fails_when_downbeat_is_non_chord() {
        // D on the downbeat over C, F on beat 3 over C.
        let f = fixture("C", "C", 1, &[(D4, 8), (F4, 8)]);
        let r = ChordTonesOnStrongBeats.score(&f.ctx(), f.rule_cfg("chord_tones_on_strong_beats"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
        assert!(r.tension > 0.0);
        assert_eq!(r.details.len(), 2);
    }
}
