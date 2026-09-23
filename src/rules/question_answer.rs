//! Rule 29: an antecedent ends open (scale degree 2, 5 or 7); the final
//! consequent ends closed (degree 1).

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::PitchClass;

pub struct QuestionAnswer;

impl Rule for QuestionAnswer {
    fn name(&self) -> &'static str {
        "question_answer"
    }
    fn params(&self) -> &'static [&'static str] {
        &["closed_antecedent_penalty"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        for bar in ctx.form.half_cadences() {
            // Bars are always filled whole, so a present bar is finished.
            if bar >= present {
                continue;
            }
            let events = ctx.bar_events(bar);
            let Some(last) = events.iter().rev().find_map(|e| e.note()) else { continue };
            n += 1;
            let deg = ctx.key.degree(PitchClass::of_midi(last.pitch));
            let s = match deg {
                Some(2) | Some(5) | Some(7) => 1.0,
                Some(1) => -cfg.param("closed_antecedent_penalty"),
                _ => 0.0,
            };
            if s < 1.0 {
                res.broken = s < 0.0;
                res.details.push(Detail {
                    bar,
                    score: s,
                    text: format!(
                        "antecedent ends on degree {} (want 2, 5 or 7)",
                        deg.map(|d| d.to_string()).unwrap_or("?".into())
                    ),
                });
            }
            sum += s;
        }
        if ctx.complete {
            if let Some(last) = ctx.notes().last() {
                n += 1;
                let closed = ctx.key.degree(PitchClass::of_midi(last.pitch)) == Some(1);
                sum += if closed { 1.0 } else { -1.0 };
            }
        }
        res.score = if n > 0 { sum / n as f32 } else { 0.0 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    // Sentence over 8 bars: bar 4 (index 3) is the half cadence.
    fn melody(bar4_end: u8, final_note: u8) -> Vec<(u8, u32)> {
        vec![
            (C4, 16), (D4, 16), (E4, 16), (bar4_end, 16),
            (C4, 16), (D4, 16), (E4, 16), (final_note, 16),
        ]
    }

    #[test]
    fn passes_open_then_closed() {
        let f = fixture("C G C G C G C C", "C", 8, &melody(D4, C4));
        let r = QuestionAnswer.score(&f.ctx(), f.rule_cfg("question_answer"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_closed_antecedent_and_open_end() {
        let f = fixture("C G C G C G C C", "C", 8, &melody(C4, D4));
        let r = QuestionAnswer.score(&f.ctx(), f.rule_cfg("question_answer"));
        assert!(r.score < 0.0, "{}", r.score);
        assert!(r.broken);
    }
}
