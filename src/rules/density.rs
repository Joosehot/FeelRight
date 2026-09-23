//! Rule 25: notes per bar should correlate with the tension target.

use super::{Context, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct Density;

impl Rule for Density {
    fn name(&self) -> &'static str {
        "density"
    }

    fn score(&self, ctx: &Context, _cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        if present < 3 {
            return RuleResult::score(0.0);
        }
        let xs: Vec<f32> = (0..present).map(|b| ctx.target(b)).collect();
        let ys: Vec<f32> = (0..present)
            .map(|b| ctx.bar_events(b).iter().filter(|e| e.note().is_some()).count() as f32)
            .collect();
        let n = present as f32;
        let mx = xs.iter().sum::<f32>() / n;
        let my = ys.iter().sum::<f32>() / n;
        let cov: f32 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        let vx: f32 = xs.iter().map(|x| (x - mx).powi(2)).sum();
        let vy: f32 = ys.iter().map(|y| (y - my).powi(2)).sum();
        if vx == 0.0 || vy == 0.0 {
            return RuleResult::score(0.0);
        }
        let corr = cov / (vx.sqrt() * vy.sqrt());
        RuleResult { score: corr, broken: corr < 0.0, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_when_busy_bars_match_high_tension() {
        let f = fixture("C C C C", "C", 4, &[(C4, 16), (E4, 8), (G4, 8), (C5, 4), (B4, 4), (A4, 4), (G4, 4), (E4, 16)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.2, 0.5, 0.9, 0.2];
        let r = Density.score(&ctx, f.rule_cfg("density"));
        assert!(r.score > 0.8, "{}", r.score);
    }

    #[test]
    fn fails_when_busy_bars_are_the_calm_ones() {
        let f = fixture("C C C C", "C", 4, &[(C4, 16), (E4, 8), (G4, 8), (C5, 4), (B4, 4), (A4, 4), (G4, 4), (E4, 16)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.9, 0.5, 0.2, 0.9];
        let r = Density.score(&ctx, f.rule_cfg("density"));
        assert!(r.score < 0.0, "{}", r.score);
    }
}
