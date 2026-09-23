//! Rule 12: tritone and augmented-2nd leaps are penalized. Breakable,
//! high tension.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::{Mode, PitchClass};

pub struct AwkwardIntervals;

impl Rule for AwkwardIntervals {
    fn name(&self) -> &'static str {
        "awkward_intervals"
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let mut res = RuleResult::default();
        let mut hits = 0;
        for w in notes.windows(2) {
            let d = w[1].pitch as i32 - w[0].pitch as i32;
            let (lo, hi) = if d < 0 { (w[1].pitch, w[0].pitch) } else { (w[0].pitch, w[1].pitch) };
            let tritone = d.abs() == 6;
            // Augmented 2nd: 3 semitones between degree 6 and the raised 7th in minor.
            let aug2 = d.abs() == 3
                && ctx.key.mode == Mode::Minor
                && PitchClass::of_midi(lo) == ctx.key.tonic.add(8)
                && PitchClass::of_midi(hi) == ctx.key.leading_tone();
            if tritone || aug2 {
                hits += 1;
                res.broken = true;
                res.tension += cfg.tension;
                res.details.push(Detail {
                    bar: ctx.bar_of(w[1].start),
                    score: -1.0,
                    text: if tritone { "tritone leap".into() } else { "augmented 2nd".into() },
                });
            }
        }
        res.score = if hits == 0 { 1.0 } else { (-(hits as f32)).max(-1.0) };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_ordinary_intervals() {
        let f = fixture("C", "C", 1, &[(C4, 4), (E4, 4), (G4, 4), (A4, 4)]);
        assert_eq!(AwkwardIntervals.score(&f.ctx(), f.rule_cfg("awkward_intervals")).score, 1.0);
    }

    #[test]
    fn fails_with_tritone_and_aug2() {
        let f = fixture("C", "C", 1, &[(F4, 8), (B4, 8)]);
        let r = AwkwardIntervals.score(&f.ctx(), f.rule_cfg("awkward_intervals"));
        assert!(r.broken);
        // A minor: F4 -> G#4
        let f = fixture("E7", "A:minor", 1, &[(F4, 8), (68, 8)]);
        let r = AwkwardIntervals.score(&f.ctx(), f.rule_cfg("awkward_intervals"));
        assert!(r.broken);
        assert_eq!(r.details[0].text, "augmented 2nd");
    }
}
