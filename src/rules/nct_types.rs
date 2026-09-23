//! Rule 2: score every non-chord tone by its type. Passing and neighbor
//! tones are fine; appoggiaturas and suspensions are expressive and pay
//! off under tension; anticipations and escape tones are mild;
//! unclassified (leap in, leap out) is penalized.

use super::analysis::{analyze, Nct};
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct NctTypes;

impl Rule for NctTypes {
    fn name(&self) -> &'static str {
        "nct_types"
    }
    fn params(&self) -> &'static [&'static str] {
        &["passing", "neighbor", "appoggiatura", "suspension", "anticipation", "escape", "unclassified", "tension_bonus"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        for info in analyze(ctx) {
            if info.nct == Nct::ChordTone {
                continue;
            }
            n += 1;
            let target = ctx.target(info.bar);
            let base = match info.nct {
                Nct::Passing => cfg.param("passing"),
                Nct::Neighbor => cfg.param("neighbor"),
                Nct::Appoggiatura => cfg.param("appoggiatura"),
                Nct::Suspension => cfg.param("suspension"),
                Nct::Anticipation => cfg.param("anticipation"),
                Nct::Escape => cfg.param("escape"),
                Nct::Unclassified | Nct::ChordTone => cfg.param("unclassified"),
            };
            // Expressive dissonances pay off where the curve asks for tension.
            let s = if matches!(info.nct, Nct::Appoggiatura | Nct::Suspension) {
                base + cfg.param("tension_bonus") * (target - 0.5) * 2.0
            } else {
                base
            };
            if s < 0.0 || matches!(info.nct, Nct::Appoggiatura | Nct::Suspension) {
                if s < 0.0 {
                    res.broken = true;
                }
                res.details.push(Detail {
                    bar: info.bar,
                    score: s,
                    text: format!(
                        "{} {} over {}",
                        info.nct.label(),
                        crate::theory::PitchClass::of_midi(info.note.pitch),
                        info.chord.map(|c| c.to_string()).unwrap_or_default()
                    ),
                });
            }
            sum += s;
        }
        res.score = if n > 0 { (sum / n as f32).clamp(-1.0, 1.0) } else { 0.0 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_passing_and_neighbor_tones() {
        let f = fixture("C", "C", 1, &[(C4, 4), (D4, 4), (E4, 4), (F4, 2), (E4, 2)]);
        let r = NctTypes.score(&f.ctx(), f.rule_cfg("nct_types"));
        assert!(r.score > 0.0, "{}", r.score);
        assert!(!r.broken);
    }

    #[test]
    fn fails_with_unclassified_dissonance() {
        let f = fixture("C", "C", 1, &[(C4, 4), (F4, 4), (C5, 4), (E4, 4)]);
        let r = NctTypes.score(&f.ctx(), f.rule_cfg("nct_types"));
        assert!(r.score < 0.0, "{}", r.score);
        assert!(r.broken);
    }
}
