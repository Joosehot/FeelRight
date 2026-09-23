//! Rule 10: a leap larger than a perfect 4th should be followed by a step
//! in the opposite direction.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct GapFill;

impl Rule for GapFill {
    fn name(&self) -> &'static str {
        "gap_fill"
    }
    fn params(&self) -> &'static [&'static str] {
        &["leap_threshold"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let iv = ctx.intervals();
        let threshold = cfg.param("leap_threshold") as i32;
        let mut res = RuleResult::default();
        let mut leaps = 0;
        let mut sum = 0.0;
        for i in 0..iv.len() {
            let leap = iv[i];
            if leap.abs() <= threshold {
                continue;
            }
            leaps += 1;
            let next = iv.get(i + 1).copied();
            let filled = matches!(next, Some(n) if matches!(n.abs(), 1 | 2) && n.signum() != leap.signum());
            if filled {
                sum += 1.0;
            } else {
                sum -= 1.0;
                res.broken = true;
                res.tension += cfg.tension;
                let bar = ctx.bar_of(notes[i + 1].start);
                res.details.push(Detail {
                    bar,
                    score: -1.0,
                    text: format!("leap of {} semitones not gap-filled", leap.abs()),
                });
            }
        }
        res.score = if leaps > 0 { sum / leaps as f32 } else { 0.0 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_when_leap_is_filled() {
        // C4 up to A4 (major 6th), then step down to G4.
        let f = fixture("C", "C", 1, &[(C4, 4), (A4, 4), (G4, 4), (E4, 4)]);
        let r = GapFill.score(&f.ctx(), f.rule_cfg("gap_fill"));
        assert_eq!(r.score, 1.0);
        assert!(!r.broken);
    }

    #[test]
    fn fails_when_leap_keeps_going() {
        // C4 up to A4, then up again to C5.
        let f = fixture("C", "C", 1, &[(C4, 4), (A4, 4), (C5, 4), (C5, 4)]);
        let r = GapFill.score(&f.ctx(), f.rule_cfg("gap_fill"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
        assert_eq!(r.details[0].bar, 0);
    }

    #[test]
    fn fourths_do_not_need_filling() {
        let f = fixture("C", "C", 1, &[(C4, 4), (F4, 4), (A4, 4), (A4, 4)]);
        let r = GapFill.score(&f.ctx(), f.rule_cfg("gap_fill"));
        assert_eq!(r.score, 0.0);
    }
}
