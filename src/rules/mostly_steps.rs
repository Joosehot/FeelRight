//! Rule 9: 60-75 % of intervals should be steps (1-2 semitones).

use super::{Context, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct MostlySteps;

impl Rule for MostlySteps {
    fn name(&self) -> &'static str {
        "mostly_steps"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_ratio", "max_ratio", "slope"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let iv = ctx.intervals();
        if iv.is_empty() {
            return RuleResult::score(0.0);
        }
        let steps = iv.iter().filter(|i| matches!(i.abs(), 1 | 2)).count();
        let ratio = steps as f32 / iv.len() as f32;
        let (lo, hi) = (cfg.param("min_ratio"), cfg.param("max_ratio"));
        let dev = if ratio < lo {
            lo - ratio
        } else if ratio > hi {
            ratio - hi
        } else {
            0.0
        };
        let score = (1.0 - cfg.param("slope") * dev).max(-1.0);
        RuleResult { score, broken: dev > 0.0, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_mostly_stepwise_line() {
        // 7 intervals: 5 steps, 2 leaps -> 0.71
        let f = fixture(
            "C",
            "C",
            2,
            &[(C4, 4), (D4, 4), (E4, 4), (G4, 4), (F4, 4), (E4, 4), (C4, 4), (D4, 4)],
        );
        let r = MostlySteps.score(&f.ctx(), f.rule_cfg("mostly_steps"));
        assert_eq!(r.score, 1.0);
        assert!(!r.broken);
    }

    #[test]
    fn fails_with_all_leaps() {
        let f = fixture("C", "C", 1, &[(C4, 4), (G4, 4), (C4, 4), (E5, 4)]);
        let r = MostlySteps.score(&f.ctx(), f.rule_cfg("mostly_steps"));
        assert!(r.score < 0.0, "{}", r.score);
        assert!(r.broken);
    }

    #[test]
    fn fails_with_only_steps() {
        let f = fixture("C", "C", 1, &[(C4, 4), (D4, 4), (E4, 4), (F4, 4)]);
        let r = MostlySteps.score(&f.ctx(), f.rule_cfg("mostly_steps"));
        assert!(r.score < 1.0);
    }
}
