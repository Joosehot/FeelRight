//! Rule 27: a bar with a Repeat role should resemble its source; a
//! transformed repeat is rewarded, an exact repeat is allowed once and
//! then penalized.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::form::Role;
use crate::motif::{is_exact, similarity};

pub struct RepetitionWithVariation;

impl Rule for RepetitionWithVariation {
    fn name(&self) -> &'static str {
        "repetition_with_variation"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_similarity", "exact_first", "exact_again"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        let mut exact_seen = 0;
        for bar in 0..present {
            let Role::Repeat { of } = ctx.form.role(bar) else { continue };
            let src = ctx.bar_events(of);
            let cur = ctx.bar_events(bar);
            if src.is_empty() || cur.is_empty() {
                continue;
            }
            n += 1;
            let s = if is_exact(&src, &cur) {
                exact_seen += 1;
                if exact_seen == 1 { cfg.param("exact_first") } else { cfg.param("exact_again") }
            } else {
                let sim = similarity(&src, &cur);
                let min = cfg.param("min_similarity");
                if sim >= min { 1.0 } else { (sim / min) * 2.0 - 1.0 }
            };
            if s < 0.0 {
                res.broken = true;
                res.details.push(Detail {
                    bar,
                    score: s,
                    text: format!("bar {} should vary bar {}", bar + 1, of + 1),
                });
            }
            sum += s;
        }
        res.score = if n > 0 { sum / n as f32 } else { 0.0 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    // Sentence form: bar 3 (index 2) repeats bar 1 (index 0).
    #[test]
    fn passes_with_varied_repeat() {
        let f = fixture(
            "C G C G",
            "C",
            4,
            &[
                (C4, 4), (D4, 4), (E4, 8), // bar 1
                (D4, 4), (E4, 4), (F4, 8), // bar 2
                (E4, 4), (F4, 4), (G4, 8), // bar 3: transposed repeat of bar 1
                (D4, 16),
            ],
        );
        let r = RepetitionWithVariation.score(&f.ctx(), f.rule_cfg("repetition_with_variation"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_with_unrelated_bar() {
        let f = fixture(
            "C G C G",
            "C",
            4,
            &[
                (C4, 4), (D4, 4), (E4, 8),
                (D4, 4), (E4, 4), (F4, 8),
                (G4, 2), (E4, 2), (C4, 2), (E4, 2), (G4, 2), (E4, 2), (C4, 4),
                (D4, 16),
            ],
        );
        let r = RepetitionWithVariation.score(&f.ctx(), f.rule_cfg("repetition_with_variation"));
        assert!(r.score < 0.0, "{}", r.score);
        assert!(r.broken);
    }
}
