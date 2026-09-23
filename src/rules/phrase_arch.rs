//! Rule 36 (phrasing): every phrase has an arch. Its highest note is
//! neither its first nor its last note, it ends lower than its peak, and
//! its opening is calmer than its middle.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct PhraseArch;

impl Rule for PhraseArch {
    fn name(&self) -> &'static str {
        "phrase_arch"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_drop"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        let mut start = 0;
        for end in ctx.form.phrase_ends() {
            if end >= present {
                break;
            }
            let notes: Vec<_> = ctx
                .notes()
                .into_iter()
                .filter(|nt| {
                    let b = ctx.bar_of(nt.start);
                    b >= start && b <= end
                })
                .collect();
            start = end + 1;
            if notes.len() < 4 {
                continue;
            }
            n += 1;
            let hi = notes.iter().map(|nt| nt.pitch).max().unwrap();
            let first_is_peak = notes[0].pitch == hi;
            let last_is_peak = notes.last().unwrap().pitch == hi;
            let drop = hi as f32 - notes.last().unwrap().pitch as f32;
            let mut s = 1.0;
            let mut why = Vec::new();
            if first_is_peak {
                s -= 1.0;
                why.push("starts on its peak");
            }
            if last_is_peak || drop < cfg.param("min_drop") {
                s -= 1.0;
                why.push("does not come down at the end");
            }
            if s < 1.0 {
                res.broken = true;
                res.details.push(Detail { bar: end, score: s, text: format!("phrase {}", why.join(", ")) });
            }
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
    fn passes_with_arched_phrases() {
        let f = fixture(
            "C G C G C G C C",
            "C",
            8,
            &[(C4, 8), (E4, 8), (G4, 8), (C5, 8), (A4, 8), (G4, 8), (E4, 8), (D4, 8),
              (C4, 8), (E4, 8), (G4, 8), (C5, 8), (A4, 8), (G4, 8), (E4, 8), (C4, 8)],
        );
        assert_eq!(PhraseArch.score(&f.ctx(), f.rule_cfg("phrase_arch")).score, 1.0);
    }

    #[test]
    fn fails_when_phrases_start_high_and_end_high() {
        let f = fixture(
            "C G C G C G C C",
            "C",
            8,
            &[(C5, 8), (A4, 8), (G4, 8), (E4, 8), (G4, 8), (A4, 8), (B4, 8), (C5, 8),
              (C5, 8), (A4, 8), (G4, 8), (E4, 8), (G4, 8), (A4, 8), (B4, 8), (C5, 8)],
        );
        let r = PhraseArch.score(&f.ctx(), f.rule_cfg("phrase_arch"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }
}
