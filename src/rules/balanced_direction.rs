//! Rule 19: total rise and total fall should roughly balance, and the
//! melody should descend into its ending.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct BalancedDirection;

impl Rule for BalancedDirection {
    fn name(&self) -> &'static str {
        "balanced_direction"
    }
    fn params(&self) -> &'static [&'static str] {
        &["descent_share", "tail_notes"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let iv = ctx.intervals();
        if iv.is_empty() {
            return RuleResult::score(0.0);
        }
        let rise: i32 = iv.iter().filter(|i| **i > 0).sum();
        let fall: i32 = -iv.iter().filter(|i| **i < 0).sum::<i32>();
        let total = (rise + fall).max(1) as f32;
        // 1 when perfectly balanced, -1 when all one direction.
        let balance = 1.0 - 2.0 * ((rise - fall).abs() as f32 / total);

        let mut res = RuleResult::default();
        let share = cfg.param("descent_share");
        if !ctx.complete {
            res.score = (1.0 - share) * balance;
            return res;
        }
        // Ending: net motion over the last `tail_notes` intervals is downward.
        let tail = cfg.param("tail_notes") as usize;
        let tail_sum: i32 = iv.iter().rev().take(tail).sum();
        let descent = if tail_sum < 0 { 1.0 } else if tail_sum == 0 { 0.0 } else { -1.0 };
        if descent < 0.0 {
            res.broken = true;
            let last = ctx.notes().last().unwrap().start;
            res.details.push(Detail {
                bar: ctx.bar_of(last),
                score: -share,
                text: "melody rises into its final note".into(),
            });
        }
        res.score = (1.0 - share) * balance + share * descent;
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_arch_shape() {
        let f = fixture("C", "C", 2, &[(C4, 4), (E4, 4), (G4, 4), (C5, 4), (G4, 4), (E4, 4), (D4, 4), (C4, 4)]);
        let r = BalancedDirection.score(&f.ctx(), f.rule_cfg("balanced_direction"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_when_only_rising() {
        let f = fixture("C", "C", 1, &[(C4, 4), (E4, 4), (G4, 4), (C5, 4)]);
        let r = BalancedDirection.score(&f.ctx(), f.rule_cfg("balanced_direction"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
