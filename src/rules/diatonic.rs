//! Rule 4: diatonic by default. Chromatic notes are allowed only as
//! leading/neighbor tones that resolve by half step.

use super::analysis::analyze;
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::PitchClass;

pub struct Diatonic;

impl Rule for Diatonic {
    fn name(&self) -> &'static str {
        "diatonic"
    }
    fn params(&self) -> &'static [&'static str] {
        &["resolved_score"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        for info in analyze(ctx) {
            let pc = PitchClass::of_midi(info.note.pitch);
            if ctx.key.is_diatonic(pc) {
                continue;
            }
            // A chromatic chord tone (e.g. G# in E7 in A minor) is the
            // harmony's business, not the melody's.
            if info.chord.map(|c| c.contains(pc)).unwrap_or(false) {
                continue;
            }
            n += 1;
            let resolved = info.next_iv.map(|b| b.abs() == 1).unwrap_or(false);
            let s = if resolved { cfg.param("resolved_score") } else { -1.0 };
            if !resolved {
                res.broken = true;
                res.tension += cfg.tension;
                res.details.push(Detail {
                    bar: info.bar,
                    score: s,
                    text: format!("chromatic {pc} does not resolve by half step"),
                });
            }
            sum += s;
        }
        res.score = if n == 0 { 1.0 } else { sum / n as f32 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_diatonic_and_resolved_chromatic() {
        // F# as a lower neighbor to G, resolving by half step.
        let f = fixture("C", "C", 1, &[(G4, 4), (66, 2), (G4, 2), (E4, 8)]);
        let r = Diatonic.score(&f.ctx(), f.rule_cfg("diatonic"));
        assert!(r.score > 0.0);
        assert!(!r.broken);
    }

    #[test]
    fn fails_with_unresolved_chromatic() {
        let f = fixture("C", "C", 1, &[(G4, 4), (66, 4), (C4, 8)]);
        let r = Diatonic.score(&f.ctx(), f.rule_cfg("diatonic"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
