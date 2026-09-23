//! Rule 33: in high-tension bars, reward postponing the resolution by a
//! beat (suspension or appoggiatura on a strong beat).

use super::analysis::{analyze, Nct};
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct DelayedResolution;

impl Rule for DelayedResolution {
    fn name(&self) -> &'static str {
        "delayed_resolution"
    }
    fn params(&self) -> &'static [&'static str] {
        &["high_threshold"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        let thr = cfg.param("high_threshold");
        let infos = analyze(ctx);
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        for bar in 0..present {
            if ctx.target(bar) < thr {
                continue;
            }
            n += 1;
            let delayed = infos
                .iter()
                .any(|i| i.bar == bar && i.strong && matches!(i.nct, Nct::Suspension | Nct::Appoggiatura));
            if delayed {
                sum += 1.0;
                res.details.push(Detail { bar, score: 1.0, text: "resolution delayed under tension".into() });
            } else {
                sum -= 0.5;
            }
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
    fn rewards_appoggiatura_in_high_tension_bar() {
        // Bar 2 over G: A on the downbeat (leap in from C), stepping to G.
        let f = fixture("C G", "C", 2, &[(C4, 16), (A4, 4), (G4, 12)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.2, 0.9];
        let r = DelayedResolution.score(&ctx, f.rule_cfg("delayed_resolution"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn penalizes_plain_chord_tone_in_high_tension_bar() {
        let f = fixture("C G", "C", 2, &[(C4, 16), (G4, 16)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.2, 0.9];
        let r = DelayedResolution.score(&ctx, f.rule_cfg("delayed_resolution"));
        assert!(r.score < 0.0);
    }
}
