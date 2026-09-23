//! Rule 11: no more than two consecutive leaps in one direction unless
//! they outline the current chord.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct ConsecutiveLeaps;

impl Rule for ConsecutiveLeaps {
    fn name(&self) -> &'static str {
        "consecutive_leaps"
    }
    fn params(&self) -> &'static [&'static str] {
        &["max_run"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let iv = ctx.intervals();
        let max_run = cfg.param("max_run") as usize;
        let mut res = RuleResult::default();
        let mut violations = 0;
        let mut run = 0usize;
        let mut dir = 0;
        for (i, &d) in iv.iter().enumerate() {
            if d.abs() > 2 && (dir == 0 || d.signum() == dir) {
                run += 1;
                dir = d.signum();
            } else {
                run = if d.abs() > 2 { 1 } else { 0 };
                dir = d.signum() * (d.abs() > 2) as i32;
            }
            if run > max_run {
                // Arpeggio exemption: all notes of the run are chord tones.
                let first = i + 1 - run;
                let chord = ctx.chord_at(notes[first].start);
                let arpeggio = chord
                    .map(|c| notes[first..=i + 1].iter().all(|n| c.contains_midi(n.pitch)))
                    .unwrap_or(false);
                if !arpeggio {
                    violations += 1;
                    res.broken = true;
                    res.tension += cfg.tension;
                    res.details.push(Detail {
                        bar: ctx.bar_of(notes[i + 1].start),
                        score: -1.0,
                        text: format!("{run} leaps in a row in one direction"),
                    });
                }
            }
        }
        res.score = if violations == 0 { 1.0 } else { (-(violations as f32)).max(-1.0) };
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_arpeggio() {
        let f = fixture("C", "C", 1, &[(C4, 4), (E4, 4), (G4, 4), (C5, 4)]);
        let r = ConsecutiveLeaps.score(&f.ctx(), f.rule_cfg("consecutive_leaps"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_with_three_non_chord_leaps() {
        let f = fixture("C", "C", 1, &[(C4, 4), (F4, 4), (A4, 4), (D5, 4)]);
        let r = ConsecutiveLeaps.score(&f.ctx(), f.rule_cfg("consecutive_leaps"));
        assert!(r.score < 0.0);
        assert!(r.broken);
    }
}
