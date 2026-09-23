//! Rule 32: surprise budget. Each interval's surprise is -log2 of its
//! probability from a fixed table; a trigram of intervals already heard
//! is discounted. Reward a moderate mean surprise.

use super::{Context, Rule, RuleResult};
use crate::config::RuleConfig;
use std::collections::HashSet;

pub struct Surprise;

const PARAMS: &[&str] = &[
    "p0", "p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9", "p10", "p11", "p12", "ngram_discount", "min_bits", "max_bits", "slope",
];

impl Rule for Surprise {
    fn name(&self) -> &'static str {
        "surprise"
    }
    fn params(&self) -> &'static [&'static str] {
        PARAMS
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let iv = ctx.intervals();
        if iv.len() < 3 {
            return RuleResult::score(0.0);
        }
        let prob = |d: i32| -> f32 {
            let k = d.abs().min(12) as usize;
            cfg.param(PARAMS[k]).max(1e-4)
        };
        let discount = cfg.param("ngram_discount");
        let mut seen: HashSet<[i32; 3]> = HashSet::new();
        let mut bits = 0.0;
        for (i, &d) in iv.iter().enumerate() {
            let mut s = -prob(d).log2();
            if i >= 2 {
                let tri = [iv[i - 2], iv[i - 1], d];
                if !seen.insert(tri) {
                    s *= discount;
                }
            }
            bits += s;
        }
        let mean = bits / iv.len() as f32;
        let (lo, hi) = (cfg.param("min_bits"), cfg.param("max_bits"));
        let slope = cfg.param("slope");
        let score = if mean < lo {
            (1.0 - slope * (lo - mean) / lo).max(-1.0)
        } else if mean > hi {
            (1.0 - slope * (mean - hi) / hi).max(-1.0)
        } else {
            1.0
        };
        RuleResult { score, broken: score < 0.0, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn moderate_melody_scores_well() {
        let f = fixture("C", "C", 2, &[(C4, 4), (D4, 4), (E4, 4), (G4, 4), (F4, 4), (E4, 4), (D4, 2), (C4, 2), (E4, 4)]);
        let r = Surprise.score(&f.ctx(), f.rule_cfg("surprise"));
        assert!(r.score > 0.0, "{}", r.score);
    }

    #[test]
    fn all_repeats_are_too_predictable() {
        let f = fixture("C", "C", 2, &[(C4, 4), (C4, 4), (C4, 4), (C4, 4), (C4, 4), (C4, 4), (C4, 4), (C4, 4)]);
        let r = Surprise.score(&f.ctx(), f.rule_cfg("surprise"));
        assert!(r.score < 0.0, "{}", r.score);
    }

    #[test]
    fn all_random_leaps_are_too_surprising() {
        let f = fixture("C", "C", 2, &[(C4, 4), (B4, 4), (C4, 4), (77, 4), (60, 4), (71, 4), (61, 4), (72, 4)]);
        let r = Surprise.score(&f.ctx(), f.rule_cfg("surprise"));
        assert!(r.score < 0.0, "{}", r.score);
    }
}
