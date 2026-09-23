//! Rule 8: across a chord change, prefer a common tone or a step.

use super::analysis::analyze;
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct SmoothChanges;

impl Rule for SmoothChanges {
    fn name(&self) -> &'static str {
        "smooth_changes"
    }
    fn params(&self) -> &'static [&'static str] {
        &["max_leap"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        for info in analyze(ctx) {
            if !info.chord_changed {
                continue;
            }
            let Some(a) = info.prev_iv else { continue };
            n += 1;
            if a.abs() <= 2 {
                sum += 1.0;
            } else if (a.abs() as f32) <= cfg.param("max_leap") {
                sum += 0.0;
            } else {
                sum -= 1.0;
                res.broken = true;
                res.tension += cfg.tension;
                res.details.push(Detail {
                    bar: info.bar,
                    score: -1.0,
                    text: format!("leap of {} semitones across the chord change", a.abs()),
                });
            }
        }
        res.score = if n == 0 { 0.0 } else { sum / n as f32 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_common_tone_and_step() {
        let f = fixture("C G Am", "C", 3, &[(G4, 16), (G4, 16), (A4, 16)]);
        let r = SmoothChanges.score(&f.ctx(), f.rule_cfg("smooth_changes"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_with_big_leaps_across_changes() {
        let f = fixture("C G Am", "C", 3, &[(C4, 16), (D5, 16), (C4, 16)]);
        let r = SmoothChanges.score(&f.ctx(), f.rule_cfg("smooth_changes"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
