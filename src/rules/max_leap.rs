//! Rule 13: leaps larger than a 6th are penalized; larger than an octave
//! is a hard violation.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct MaxLeap;

impl Rule for MaxLeap {
    fn name(&self) -> &'static str {
        "max_leap"
    }
    fn params(&self) -> &'static [&'static str] {
        &["soft_max", "hard_max"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let (soft, hard) = (cfg.param("soft_max") as i32, cfg.param("hard_max") as i32);
        let mut res = RuleResult::default();
        let mut hits = 0;
        for w in notes.windows(2) {
            let d = (w[1].pitch as i32 - w[0].pitch as i32).abs();
            if d > hard {
                res.hard_violation = true;
            }
            if d > soft {
                hits += 1;
                res.broken = true;
                res.tension += cfg.tension;
                res.details.push(Detail {
                    bar: ctx.bar_of(w[1].start),
                    score: -1.0,
                    text: format!("leap of {d} semitones"),
                });
            }
        }
        res.score = if hits == 0 { 1.0 } else { (-(hits as f32)).max(-1.0) };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_sixth() {
        let f = fixture("C", "C", 1, &[(C4, 8), (A4, 8)]);
        assert_eq!(MaxLeap.score(&f.ctx(), f.rule_cfg("max_leap")).score, 1.0);
    }

    #[test]
    fn fails_with_ninth() {
        let f = fixture("C", "C", 1, &[(C4, 8), (D5, 8)]);
        let r = MaxLeap.score(&f.ctx(), f.rule_cfg("max_leap"));
        assert!(r.hard_violation);
        assert!(r.broken);
    }
}
