//! Rule 20: the opening rhythm returns. Reward a recognizable share of
//! bars that reuse the first bar's rhythm (exact, retrograde or halved).

use super::{Context, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::motif::rhythm_of;

pub struct RhythmicMotif;

fn related(a: &[u32], b: &[u32]) -> bool {
    if a == b {
        return true;
    }
    let mut r = a.to_vec();
    r.reverse();
    if r == b {
        return true;
    }
    // Diminution: b starts with a halved.
    let half: Vec<u32> = a.iter().map(|d| d / 2).filter(|d| *d > 0).collect();
    !half.is_empty() && b.starts_with(&half)
}

impl Rule for RhythmicMotif {
    fn name(&self) -> &'static str {
        "rhythmic_motif"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_share", "max_share"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        if present < 2 {
            return RuleResult::score(0.0);
        }
        let opening = rhythm_of(&ctx.bar_events(0));
        let mut reused = 0;
        for bar in 1..present {
            if related(&opening, &rhythm_of(&ctx.bar_events(bar))) {
                reused += 1;
            }
        }
        let share = reused as f32 / (present - 1) as f32;
        let (lo, hi) = (cfg.param("min_share"), cfg.param("max_share"));
        let score = if share < lo {
            (share / lo) * 2.0 - 1.0
        } else if share > hi {
            1.0 - (share - hi) / (1.0 - hi)
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
    fn passes_when_rhythm_returns() {
        let f = fixture("C G C G", "C", 4, &[(C4, 4), (D4, 4), (E4, 8), (D4, 4), (E4, 4), (F4, 8), (E4, 16), (C4, 16)]);
        let r = RhythmicMotif.score(&f.ctx(), f.rule_cfg("rhythmic_motif"));
        assert!(r.score > 0.0, "{}", r.score);
    }

    #[test]
    fn fails_when_every_bar_differs() {
        let f = fixture("C G C G", "C", 4, &[(C4, 4), (D4, 4), (E4, 8), (D4, 2), (E4, 2), (F4, 12), (E4, 16), (C4, 8), (C4, 8)]);
        let r = RhythmicMotif.score(&f.ctx(), f.rule_cfg("rhythmic_motif"));
        assert!(r.score < 0.0, "{}", r.score);
    }
}
