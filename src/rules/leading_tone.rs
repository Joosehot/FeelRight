//! Rule 5: the leading tone resolves up to the tonic. Strict in cadence
//! bars, soft elsewhere.

use super::analysis::analyze;
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::form::Role;
use crate::theory::PitchClass;

pub struct LeadingTone;

impl Rule for LeadingTone {
    fn name(&self) -> &'static str {
        "leading_tone"
    }
    fn params(&self) -> &'static [&'static str] {
        &["soft_penalty"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let lt = ctx.key.leading_tone();
        let mut sum = 0.0;
        let mut n = 0;
        for info in analyze(ctx) {
            if PitchClass::of_midi(info.note.pitch) != lt {
                continue;
            }
            let Some(b) = info.next_iv else {
                if ctx.complete {
                    // Ending on the leading tone: worst case.
                    n += 1;
                    sum -= 1.0;
                    res.broken = true;
                    res.details.push(Detail { bar: info.bar, score: -1.0, text: "melody ends on the leading tone".into() });
                }
                continue;
            };
            n += 1;
            if b == 1 {
                sum += 1.0;
                continue;
            }
            let cadence = matches!(ctx.form.role(info.bar), Role::Cadence(_));
            let s = if cadence { -1.0 } else { -cfg.param("soft_penalty") };
            res.broken = true;
            res.tension += cfg.tension;
            res.details.push(Detail {
                bar: info.bar,
                score: s,
                text: format!("leading tone {lt} does not rise to the tonic{}", if cadence { " at the cadence" } else { "" }),
            });
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
    fn passes_when_b_rises_to_c() {
        let f = fixture("G C", "C", 2, &[(D5, 8), (B4, 8), (C5, 16)]);
        let r = LeadingTone.score(&f.ctx(), f.rule_cfg("leading_tone"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_when_b_falls_at_cadence() {
        // 2 bars: bar 2 is the final cadence bar.
        let f = fixture("G C", "C", 2, &[(D5, 16), (B4, 8), (G4, 8)]);
        let r = LeadingTone.score(&f.ctx(), f.rule_cfg("leading_tone"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
