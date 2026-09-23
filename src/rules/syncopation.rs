//! Rule 23: syncopation (a note starting off the beat and lasting across
//! the next beat) is penalized at low tension and rewarded at high tension.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct Syncopation;

pub fn is_syncopated(ctx: &Context, start: u32, dur: u32) -> bool {
    let spb = ctx.meter.steps_per_beat();
    let off = start % spb != 0;
    let to_next_beat = spb - start % spb;
    off && dur > to_next_beat
}

impl Rule for Syncopation {
    fn name(&self) -> &'static str {
        "syncopation"
    }
    fn params(&self) -> &'static [&'static str] {
        &["threshold"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        let thr = cfg.param("threshold");
        for note in ctx.notes() {
            if !is_syncopated(ctx, note.start, note.dur) {
                continue;
            }
            n += 1;
            let bar = ctx.bar_of(note.start);
            let t = ctx.target(bar);
            // -1 at target 0, +1 at target 1, zero at the threshold.
            let s = if t >= thr { (t - thr) / (1.0 - thr) } else { -(thr - t) / thr };
            if s < 0.0 {
                res.broken = true;
                res.tension += cfg.tension;
            }
            res.details.push(Detail { bar, score: s, text: "syncopation".into() });
            sum += s;
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
    fn syncopation_pays_off_under_high_tension() {
        let mut f = fixture("C", "C", 1, &[(C4, 2), (E4, 6), (G4, 8)]);
        // Default arch for 1 bar is 0.5 -> neutral-ish; override to high.
        let ctx_low = f.ctx();
        let low = Syncopation.score(&ctx_low, f.rule_cfg("syncopation"));
        drop(ctx_low);
        f.bars = 1;
        let high = {
            let mut ctx = f.ctx();
            ctx.tension = &[1.0];
            Syncopation.score(&ctx, f.rule_cfg("syncopation"))
        };
        assert!(high.score > low.score);
        assert_eq!(high.score, 1.0);
    }

    #[test]
    fn syncopation_penalized_under_low_tension() {
        let f = fixture("C", "C", 1, &[(C4, 2), (E4, 6), (G4, 8)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.0];
        let r = Syncopation.score(&ctx, f.rule_cfg("syncopation"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
