//! Rule 34: after the tension maximum, reach a stable degree (1, 3 or 5)
//! on a strong beat within the next two bars.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::PitchClass;

pub struct ResolveAfterPeak;

/// Bar with the highest target tension (first if tied).
pub fn peak_bar(ctx: &Context) -> Option<u32> {
    (0..ctx.bars)
        .map(|b| (b, ctx.target(b)))
        .fold(None, |best: Option<(u32, f32)>, (b, t)| match best {
            Some((_, bt)) if bt >= t => best,
            _ => Some((b, t)),
        })
        .map(|(b, _)| b)
}

impl Rule for ResolveAfterPeak {
    fn name(&self) -> &'static str {
        "resolve_after_peak"
    }
    fn params(&self) -> &'static [&'static str] {
        &["window_bars"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let Some(peak) = peak_bar(ctx) else { return RuleResult::score(0.0) };
        let window = cfg.param("window_bars") as u32;
        let last = (peak + window).min(ctx.bars - 1);
        let present = ctx.bars_present();
        if peak + 1 >= present && !ctx.complete {
            return RuleResult::score(0.0);
        }
        if last >= present && !ctx.complete {
            // Window not fully present: judge what is there without penalty.
        }
        let stable = ctx.notes().iter().any(|n| {
            let b = ctx.bar_of(n.start);
            b > peak && b <= last && ctx.is_strong(n.start)
                && matches!(ctx.key.degree(PitchClass::of_midi(n.pitch)), Some(1) | Some(3) | Some(5))
        });
        if stable {
            return RuleResult::score(1.0);
        }
        if last >= present && !ctx.complete {
            return RuleResult::score(0.0);
        }
        RuleResult {
            score: -1.0,
            broken: true,
            details: vec![Detail { bar: last, score: -1.0, text: "no stable degree after the tension peak".into() }],
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_when_tonic_follows_peak() {
        let f = fixture("C G C C", "C", 4, &[(E4, 16), (A4, 16), (C5, 16), (C4, 16)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.2, 0.9, 0.4, 0.2];
        assert_eq!(ResolveAfterPeak.score(&ctx, f.rule_cfg("resolve_after_peak")).score, 1.0);
    }

    #[test]
    fn fails_when_unstable_after_peak() {
        let f = fixture("C G C C", "C", 4, &[(E4, 16), (A4, 16), (D4, 16), (F4, 16)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.2, 0.9, 0.4, 0.2];
        let r = ResolveAfterPeak.score(&ctx, f.rule_cfg("resolve_after_peak"));
        assert_eq!(r.score, -1.0);
    }
}
