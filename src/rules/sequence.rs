//! Rule 28: in Fragment bars, a transposed restatement of the previous
//! bar (a sequence following the harmony) is rewarded.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::form::Role;
use crate::motif::{is_exact, is_transposition, pitches_of, similarity};

pub struct Sequence;

impl Rule for Sequence {
    fn name(&self) -> &'static str {
        "sequence"
    }
    fn params(&self) -> &'static [&'static str] {
        &["partial_credit"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        for bar in 1..present {
            if !matches!(ctx.form.role(bar), Role::Fragment { .. }) {
                continue;
            }
            let prev = ctx.bar_events(bar - 1);
            let cur = ctx.bar_events(bar);
            if prev.is_empty() || cur.is_empty() {
                continue;
            }
            n += 1;
            let s = if is_transposition(&prev, &cur) && !is_exact(&prev, &cur) {
                1.0
            } else if similarity(&prev, &cur) >= 0.75
                && pitches_of(&prev)[0] != pitches_of(&cur)[0]
            {
                cfg.param("partial_credit")
            } else {
                -0.5
            };
            if s < 0.0 {
                res.details.push(Detail {
                    bar,
                    score: s,
                    text: format!("bar {} does not sequence bar {}", bar + 1, bar),
                });
            }
            sum += s;
        }
        res.score = if n > 0 { sum / n as f32 } else { 0.0 };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    // Sentence form over 8 bars: bars 5 and 6 (index 4, 5) are Fragments.
    fn base() -> Vec<(u8, u32)> {
        let mut v = vec![(C4, 8), (E4, 8), (D4, 8), (F4, 8), (C4, 8), (E4, 8), (D4, 16)];
        v.push((E4, 4)); // bar 5 start
        v
    }

    #[test]
    fn passes_with_exact_sequence() {
        let mut v = base();
        v.extend([(F4, 4), (G4, 8)]); // bar 5: E F G
        v.extend([(F4, 4), (G4, 4), (A4, 8)]); // bar 6: F G A (transposed)
        v.extend([(G4, 16), (C4, 16)]);
        let f = fixture("C G Am F C G Am F", "C", 8, &v);
        let r = Sequence.score(&f.ctx(), f.rule_cfg("sequence"));
        assert!(r.score > 0.0, "{}", r.score);
    }

    #[test]
    fn fails_with_unrelated_fragment() {
        let mut v = base();
        v.extend([(F4, 4), (G4, 8)]); // bar 5
        v.extend([(C5, 2), (C5, 2), (C5, 2), (C5, 2), (G4, 8)]); // bar 6 unrelated
        v.extend([(G4, 16), (C4, 16)]);
        let f = fixture("C G Am F C G Am F", "C", 8, &v);
        let r = Sequence.score(&f.ctx(), f.rule_cfg("sequence"));
        assert!(r.score < 0.0, "{}", r.score);
    }
}
