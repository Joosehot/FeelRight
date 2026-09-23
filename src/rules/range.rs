//! Rule 15: stay within an octave plus a third; hard cap at an octave plus
//! a fifth.

use super::{Context, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct Range;

impl Rule for Range {
    fn name(&self) -> &'static str {
        "range"
    }
    fn params(&self) -> &'static [&'static str] {
        &["soft_max", "hard_max"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let Some(lo) = notes.iter().map(|n| n.pitch).min() else {
            return RuleResult::score(0.0);
        };
        let hi = notes.iter().map(|n| n.pitch).max().unwrap();
        let span = (hi - lo) as f32;
        let (soft, hard) = (cfg.param("soft_max"), cfg.param("hard_max"));
        if span <= soft {
            return RuleResult::score(1.0);
        }
        if span > hard {
            return RuleResult {
                score: -1.0,
                broken: true,
                hard_violation: true,
                ..Default::default()
            };
        }
        // Linear from 1 at soft to -1 at hard.
        let t = (span - soft) / (hard - soft);
        RuleResult { score: 1.0 - 2.0 * t, broken: true, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_within_a_tenth() {
        let f = fixture("C", "C", 1, &[(C4, 4), (E5, 4), (G4, 4), (C4, 4)]);
        let r = Range.score(&f.ctx(), f.rule_cfg("range"));
        assert_eq!(r.score, 1.0);
        assert!(!r.hard_violation);
    }

    #[test]
    fn soft_penalty_between_limits() {
        // C4..F5 = 17 semitones
        let f = fixture("C", "C", 1, &[(C4, 4), (F5, 4), (G4, 4), (C4, 4)]);
        let r = Range.score(&f.ctx(), f.rule_cfg("range"));
        assert!(r.score < 1.0 && r.score > -1.0);
        assert!(!r.hard_violation);
    }

    #[test]
    fn fails_hard_beyond_octave_and_fifth() {
        // C4..A5 = 21 semitones
        let f = fixture("C", "C", 1, &[(C4, 4), (A5, 4), (G4, 4), (C4, 4)]);
        let r = Range.score(&f.ctx(), f.rule_cfg("range"));
        assert!(r.hard_violation);
    }
}
