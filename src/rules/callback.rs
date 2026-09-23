//! Rule 31: the opening motif returns (possibly transformed) in the last
//! phrase.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::motif::similarity;

pub struct Callback;

impl Rule for Callback {
    fn name(&self) -> &'static str {
        "callback"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_similarity"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        if !ctx.complete || ctx.bars < 4 {
            return RuleResult::score(0.0);
        }
        let opening = ctx.bar_events(0);
        if opening.is_empty() {
            return RuleResult::score(0.0);
        }
        // Last phrase, excluding the final cadence bar.
        let start = ctx.form.phrase_start(ctx.bars - 1);
        let end = ctx.bars - 1;
        let mut best = 0.0f32;
        for bar in start..end {
            let ev = ctx.bar_events(bar);
            if !ev.is_empty() {
                best = best.max(similarity(&opening, &ev));
            }
        }
        // The opening bar itself would trivially match in a 1-phrase piece.
        if start == 0 {
            return RuleResult::score(0.0);
        }
        let min = cfg.param("min_similarity");
        let score = if best >= min { 1.0 } else { (best / min) * 2.0 - 1.0 };
        let mut res = RuleResult::score(score);
        if score < 0.0 {
            res.broken = true;
            res.details.push(Detail {
                bar: end,
                score,
                text: "opening motif does not return in the last phrase".into(),
            });
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_when_opening_returns() {
        let mut v = vec![(C4, 4), (D4, 4), (E4, 8)]; // bar 1
        v.extend([(G4, 16), (G4, 16), (D4, 16)]); // bars 2-4
        v.extend([(E4, 4), (F4, 4), (G4, 8)]); // bar 5 varied return
        v.extend([(A4, 16), (G4, 16), (C4, 16)]);
        let f = fixture("C G C G C G C C", "C", 8, &v);
        let r = Callback.score(&f.ctx(), f.rule_cfg("callback"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_without_return() {
        let mut v = vec![(C4, 4), (D4, 4), (E4, 8)];
        v.extend([(G4, 16), (G4, 16), (D4, 16)]);
        v.extend([(G4, 2), (F4, 2), (E4, 2), (D4, 2), (C4, 8)]);
        v.extend([(A4, 16), (G4, 16), (C4, 16)]);
        let f = fixture("C G C G C G C C", "C", 8, &v);
        let r = Callback.score(&f.ctx(), f.rule_cfg("callback"));
        assert!(r.score < 0.0, "{}", r.score);
    }
}
