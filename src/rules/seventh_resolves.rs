//! Rule 6: a chordal 7th resolves down by step when the chord changes.

use super::analysis::analyze;
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::PitchClass;

pub struct SeventhResolves;

impl Rule for SeventhResolves {
    fn name(&self) -> &'static str {
        "seventh_resolves"
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        for info in analyze(ctx) {
            let Some(chord) = info.chord else { continue };
            let Some(seventh) = chord.seventh() else { continue };
            if PitchClass::of_midi(info.note.pitch) != seventh || !info.chord_changes_next {
                continue;
            }
            let Some(b) = info.next_iv else { continue };
            n += 1;
            if b == -1 || b == -2 {
                sum += 1.0;
            } else {
                sum -= 1.0;
                res.broken = true;
                res.tension += cfg.tension;
                res.details.push(Detail {
                    bar: info.bar,
                    score: -1.0,
                    text: format!("7th of {chord} does not resolve down by step"),
                });
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
    fn passes_when_f_falls_to_e() {
        let f = fixture("G7 C", "C", 2, &[(G4, 8), (F4, 8), (E4, 16)]);
        let r = SeventhResolves.score(&f.ctx(), f.rule_cfg("seventh_resolves"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_when_f_leaps_up() {
        let f = fixture("G7 C", "C", 2, &[(G4, 8), (F4, 8), (C5, 16)]);
        let r = SeventhResolves.score(&f.ctx(), f.rule_cfg("seventh_resolves"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
