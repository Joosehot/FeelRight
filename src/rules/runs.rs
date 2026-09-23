//! Rule 18: more than N notes in the same direction is penalized.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct Runs;

impl Rule for Runs {
    fn name(&self) -> &'static str {
        "runs"
    }
    fn params(&self) -> &'static [&'static str] {
        &["max_notes"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let iv = ctx.intervals();
        let max = cfg.param("max_notes") as usize;
        let mut res = RuleResult::default();
        let mut hits = 0;
        let mut run = 1usize;
        let mut dir = 0;
        for (i, &d) in iv.iter().enumerate() {
            let s = d.signum();
            if s != 0 && s == dir {
                run += 1;
            } else if s != 0 {
                run = 2;
                dir = s;
            } else {
                continue;
            }
            if run == max + 1 {
                hits += 1;
                res.broken = true;
                res.details.push(Detail {
                    bar: ctx.bar_of(notes[i + 1].start),
                    score: -1.0,
                    text: format!("more than {max} notes in one direction"),
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
    fn passes_with_a_scale_fragment() {
        let f = fixture("C", "C", 1, &[(C4, 2), (D4, 2), (E4, 2), (F4, 2), (G4, 4), (E4, 4)]);
        assert_eq!(Runs.score(&f.ctx(), f.rule_cfg("runs")).score, 1.0);
    }

    #[test]
    fn fails_with_an_octave_scale() {
        let f = fixture("C", "C", 1, &[(C4, 2), (D4, 2), (E4, 2), (F4, 2), (G4, 2), (A4, 2), (B4, 2), (C5, 2)]);
        let r = Runs.score(&f.ctx(), f.rule_cfg("runs"));
        assert!(r.broken);
    }
}
