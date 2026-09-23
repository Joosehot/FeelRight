//! Rule 16: more than N identical consecutive pitches is penalized unless
//! the run repeats a rhythmic cell.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct Monotone;

impl Rule for Monotone {
    fn name(&self) -> &'static str {
        "monotone"
    }
    fn params(&self) -> &'static [&'static str] {
        &["max_repeats"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let max = cfg.param("max_repeats") as usize;
        let mut res = RuleResult::default();
        let mut hits = 0;
        let mut i = 0;
        while i < notes.len() {
            let mut j = i;
            while j + 1 < notes.len() && notes[j + 1].pitch == notes[i].pitch {
                j += 1;
            }
            let len = j - i + 1;
            if len > max {
                // Rhythmic-motif exemption: durations form a repeating cell.
                let durs: Vec<u32> = notes[i..=j].iter().map(|n| n.dur).collect();
                let cell = (1..=len / 2).any(|c| len % c == 0 && durs.chunks(c).all(|ch| ch == &durs[..c]) && c < len);
                if !cell || durs.iter().all(|d| *d == durs[0]) {
                    hits += 1;
                    res.broken = true;
                    res.details.push(Detail {
                        bar: ctx.bar_of(notes[i].start),
                        score: -1.0,
                        text: format!("{len} repeated notes"),
                    });
                }
            }
            i = j + 1;
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
    fn passes_with_three_repeats() {
        let f = fixture("C", "C", 1, &[(C4, 4), (C4, 4), (C4, 4), (E4, 4)]);
        assert_eq!(Monotone.score(&f.ctx(), f.rule_cfg("monotone")).score, 1.0);
    }

    #[test]
    fn fails_with_five_even_repeats() {
        let f = fixture("C", "C", 2, &[(C4, 4), (C4, 4), (C4, 4), (C4, 4), (C4, 16)]);
        let r = Monotone.score(&f.ctx(), f.rule_cfg("monotone"));
        assert!(r.broken);
    }
}
