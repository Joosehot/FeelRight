//! Rule 14: the highest note appears once, around 60-75 % through the
//! melody.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct SingleClimax;

impl Rule for SingleClimax {
    fn name(&self) -> &'static str {
        "single_climax"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_pos", "max_pos", "multi_penalty", "min_height"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        if notes.len() < 2 {
            return RuleResult::score(0.0);
        }
        let hi = notes.iter().map(|n| n.pitch).max().unwrap();
        let lo = notes.iter().map(|n| n.pitch).min().unwrap();
        let peaks: Vec<_> = notes.iter().filter(|n| n.pitch == hi).collect();
        let total = (ctx.bars * ctx.meter.steps_per_bar()) as f32;
        let pos = peaks[0].start as f32 / total;
        let (min_pos, max_pos) = (cfg.param("min_pos"), cfg.param("max_pos"));
        let mut res = RuleResult::default();
        let bar = ctx.bar_of(peaks[0].start);

        // Placement: inside the window is good; too late is bad; too early
        // is only judged once the melody is complete (a later peak may come).
        let placement = if pos > max_pos {
            -1.0
        } else if pos >= min_pos {
            1.0
        } else if ctx.complete {
            -1.0 + 2.0 * (pos / min_pos)
        } else {
            0.0
        };
        if ctx.complete && placement < 1.0 {
            res.details.push(Detail {
                bar,
                score: placement,
                text: format!("climax at {:.0}% of the melody", pos * 100.0),
            });
        }

        // Uniqueness.
        let extra = (peaks.len() as f32 - 1.0) * cfg.param("multi_penalty");
        if peaks.len() > 1 {
            res.broken = true;
            res.details.push(Detail {
                bar: ctx.bar_of(peaks[1].start),
                score: -extra,
                text: format!("highest note repeated {} times", peaks.len()),
            });
        }

        // Height: a climax needs room below it.
        let height = if ctx.complete && (hi - lo) as f32 <= cfg.param("min_height") { -0.5 } else { 0.0 };

        res.score = (placement - extra + height).clamp(-1.0, 1.0);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_one_peak_in_window() {
        // 2 bars = 32 steps; peak G5 at step 20 (62 %).
        let f = fixture("C", "C", 2, &[(C4, 4), (E4, 4), (G4, 8), (C5, 4), (G5, 4), (E5, 4), (C5, 4)]);
        let r = SingleClimax.score(&f.ctx(), f.rule_cfg("single_climax"));
        assert_eq!(r.score, 1.0);
        assert!(!r.broken);
    }

    #[test]
    fn fails_with_repeated_peak_at_start() {
        let f = fixture("C", "C", 2, &[(G5, 4), (E4, 4), (G5, 8), (C4, 4), (E4, 4), (D4, 4), (C4, 4)]);
        let r = SingleClimax.score(&f.ctx(), f.rule_cfg("single_climax"));
        assert!(r.score < 0.0, "{}", r.score);
        assert!(r.broken);
    }
}
