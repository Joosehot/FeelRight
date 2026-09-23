//! Rule 17: alternating between two pitches for more than N notes is
//! penalized.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct Noodling;

impl Rule for Noodling {
    fn name(&self) -> &'static str {
        "noodling"
    }
    fn params(&self) -> &'static [&'static str] {
        &["max_alternations"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let max = cfg.param("max_alternations") as usize;
        let mut res = RuleResult::default();
        let mut hits = 0;
        let mut i = 0;
        while i + 1 < notes.len() {
            let (a, b) = (notes[i].pitch, notes[i + 1].pitch);
            if a == b {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j + 1 < notes.len() && notes[j + 1].pitch == notes[j - 1].pitch {
                j += 1;
            }
            let len = j - i + 1;
            if len > max {
                hits += 1;
                res.broken = true;
                res.details.push(Detail {
                    bar: ctx.bar_of(notes[i].start),
                    score: -1.0,
                    text: format!("{len} notes alternating between two pitches"),
                });
            }
            i = j;
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
    fn passes_with_short_alternation() {
        let f = fixture("C", "C", 1, &[(C4, 4), (D4, 4), (C4, 4), (E4, 4)]);
        assert_eq!(Noodling.score(&f.ctx(), f.rule_cfg("noodling")).score, 1.0);
    }

    #[test]
    fn fails_with_long_alternation() {
        let f = fixture("C", "C", 2, &[(C4, 4), (D4, 4), (C4, 4), (D4, 4), (C4, 4), (D4, 4), (C4, 8)]);
        let r = Noodling.score(&f.ctx(), f.rule_cfg("noodling"));
        assert!(r.broken);
    }
}
