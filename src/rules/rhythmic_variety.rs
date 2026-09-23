//! Rule 21: a phrase should not use one duration only.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use std::collections::HashSet;

pub struct RhythmicVariety;

impl Rule for RhythmicVariety {
    fn name(&self) -> &'static str {
        "rhythmic_variety"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_distinct"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        let min = cfg.param("min_distinct") as usize;
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        let mut start = 0;
        let mut ends = ctx.form.phrase_ends();
        if ends.last().copied() != Some(ctx.bars - 1) {
            ends.push(ctx.bars - 1);
        }
        for end in ends {
            if end >= present && !(ctx.complete) {
                break;
            }
            let end = end.min(present - 1);
            let mut durs = HashSet::new();
            for bar in start..=end {
                for e in ctx.bar_events(bar) {
                    durs.insert(e.dur());
                }
            }
            if !durs.is_empty() {
                n += 1;
                if durs.len() >= min {
                    sum += 1.0;
                } else {
                    sum -= 1.0;
                    res.broken = true;
                    res.details.push(Detail { bar: end, score: -1.0, text: "phrase uses a single duration".into() });
                }
            }
            start = end + 1;
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
    fn passes_with_mixed_durations() {
        let f = fixture("C", "C", 1, &[(C4, 4), (D4, 2), (E4, 2), (G4, 8)]);
        assert_eq!(RhythmicVariety.score(&f.ctx(), f.rule_cfg("rhythmic_variety")).score, 1.0);
    }

    #[test]
    fn fails_with_all_quarters() {
        let f = fixture("C", "C", 1, &[(C4, 4), (D4, 4), (E4, 4), (G4, 4)]);
        let r = RhythmicVariety.score(&f.ctx(), f.rule_cfg("rhythmic_variety"));
        assert_eq!(r.score, -1.0);
    }
}
