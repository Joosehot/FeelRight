//! Rule 35: the bar after the tension maximum carries the strongest
//! resolution of the phrase: a chord tone on the downbeat, reached by
//! step, held long.

use super::resolve_after_peak::peak_bar;
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct Arrival;

impl Rule for Arrival {
    fn name(&self) -> &'static str {
        "arrival"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_dur"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let Some(peak) = peak_bar(ctx) else { return RuleResult::score(0.0) };
        let bar = peak + 1;
        if bar >= ctx.bars || bar >= ctx.bars_present() {
            return RuleResult::score(0.0);
        }
        let notes = ctx.notes();
        let spb = ctx.meter.steps_per_bar();
        let Some(idx) = notes.iter().position(|n| n.start == bar * spb) else {
            return RuleResult {
                score: -1.0,
                broken: true,
                details: vec![Detail { bar, score: -1.0, text: "no note on the arrival downbeat".into() }],
                ..Default::default()
            };
        };
        let n = notes[idx];
        let chord_tone = ctx.chord_at(n.start).map(|c| c.contains_midi(n.pitch)).unwrap_or(false);
        let by_step = idx > 0 && matches!((n.pitch as i32 - notes[idx - 1].pitch as i32).abs(), 1 | 2);
        let long = n.dur as f32 >= cfg.param("min_dur");
        let s = (chord_tone as i32 + by_step as i32 + long as i32) as f32 / 3.0 * 2.0 - 1.0;
        let mut res = RuleResult::score(s);
        if s < 1.0 {
            res.details.push(Detail {
                bar,
                score: s,
                text: format!(
                    "arrival after the peak: {}{}{}",
                    if chord_tone { "" } else { "not a chord tone, " },
                    if by_step { "" } else { "not by step, " },
                    if long { "" } else { "short" }
                ),
            });
        }
        if s < 0.0 {
            res.broken = true;
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_stepwise_long_chord_tone() {
        let f = fixture("C G C C", "C", 4, &[(E4, 16), (F4, 8), (D5, 8), (C5, 16), (C4, 16)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.2, 0.9, 0.4, 0.2];
        assert_eq!(Arrival.score(&ctx, f.rule_cfg("arrival")).score, 1.0);
    }

    #[test]
    fn fails_with_leap_to_short_non_chord_tone() {
        let f = fixture("C G C C", "C", 4, &[(E4, 16), (G4, 16), (D4, 2), (E4, 14), (C4, 16)]);
        let mut ctx = f.ctx();
        ctx.tension = &[0.2, 0.9, 0.4, 0.2];
        let r = Arrival.score(&ctx, f.rule_cfg("arrival"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
