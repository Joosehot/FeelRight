//! Rule 30: the final note is the tonic (or the 3rd of the tonic chord)
//! and starts on a strong beat.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::{Mode, PitchClass};

pub struct FinalNote;

impl Rule for FinalNote {
    fn name(&self) -> &'static str {
        "final_note"
    }
    fn params(&self) -> &'static [&'static str] {
        &["third_score", "weak_beat_penalty"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let Some(last) = notes.last() else {
            return RuleResult::score(0.0);
        };
        if !ctx.complete {
            return RuleResult::score(0.0);
        }
        let pc = PitchClass::of_midi(last.pitch);
        let tonic = ctx.key.tonic;
        let third = match ctx.key.mode {
            Mode::Major => tonic.add(4),
            Mode::Minor => tonic.add(3),
        };
        let mut res = RuleResult::default();
        let bar = ctx.bar_of(last.start);
        res.score = if pc == tonic {
            1.0
        } else if pc == third {
            cfg.param("third_score")
        } else {
            res.broken = true;
            res.details.push(Detail {
                bar,
                score: -1.0,
                text: format!("final note {pc} is not the tonic {tonic}"),
            });
            -1.0
        };
        if !ctx.is_strong(last.start) {
            res.score -= cfg.param("weak_beat_penalty");
            res.broken = true;
            res.details.push(Detail {
                bar,
                score: -cfg.param("weak_beat_penalty"),
                text: "final note starts on a weak beat".into(),
            });
        }
        res.score = res.score.max(-1.0);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_on_tonic_downbeat() {
        let f = fixture("G C", "C", 2, &[(B4, 8), (D5, 8), (C5, 16)]);
        let r = FinalNote.score(&f.ctx(), f.rule_cfg("final_note"));
        assert_eq!(r.score, 1.0);
        assert!(!r.broken);
    }

    #[test]
    fn third_scores_partially() {
        let f = fixture("G C", "C", 2, &[(B4, 8), (D5, 8), (E5, 16)]);
        let r = FinalNote.score(&f.ctx(), f.rule_cfg("final_note"));
        assert_eq!(r.score, 0.5);
    }

    #[test]
    fn fails_on_non_tonic_weak_beat() {
        // Ends on D on the "and" of beat 2.
        let f = fixture("G C", "C", 2, &[(B4, 8), (D5, 8), (C5, 6), (D5, 10)]);
        let r = FinalNote.score(&f.ctx(), f.rule_cfg("final_note"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
        assert_eq!(r.details.len(), 2);
    }

    #[test]
    fn minor_key_third_is_minor() {
        let f = fixture("Am", "A:minor", 1, &[(A4, 8), (C5, 8)]);
        let r = FinalNote.score(&f.ctx(), f.rule_cfg("final_note"));
        assert_eq!(r.score, 0.5);
    }
}
