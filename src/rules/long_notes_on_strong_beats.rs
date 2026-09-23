//! Rule 22: long notes sit on strong beats, and the final note is long.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;

pub struct LongNotesOnStrongBeats;

impl Rule for LongNotesOnStrongBeats {
    fn name(&self) -> &'static str {
        "long_notes_on_strong_beats"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_final_dur", "final_share"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let notes = ctx.notes();
        let Some(last) = notes.last() else {
            return RuleResult::score(0.0);
        };
        let mut res = RuleResult::default();

        // Part A: mean duration on strong beats vs. on weak positions.
        let (mut strong_sum, mut strong_n, mut weak_sum, mut weak_n) = (0.0, 0, 0.0, 0);
        for n in &notes {
            if ctx.is_strong(n.start) {
                strong_sum += n.dur as f32;
                strong_n += 1;
            } else {
                weak_sum += n.dur as f32;
                weak_n += 1;
            }
        }
        let placement = if strong_n > 0 && weak_n > 0 {
            let s = strong_sum / strong_n as f32;
            let w = weak_sum / weak_n as f32;
            ((s - w) / ctx.meter.steps_per_beat() as f32).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        if placement < 0.0 {
            res.broken = true;
            res.tension += cfg.tension;
        }

        // Part B: final note is long (only once the melody is complete).
        if !ctx.complete {
            res.score = placement;
            return res;
        }
        let final_ok = last.dur as f32 >= cfg.param("min_final_dur");
        let final_score = if final_ok { 1.0 } else { -1.0 };
        if !final_ok {
            res.broken = true;
            res.details.push(Detail {
                bar: ctx.bar_of(last.start),
                score: -1.0,
                text: format!("final note is short ({} steps)", last.dur),
            });
        }

        let share = cfg.param("final_share");
        res.score = (1.0 - share) * placement + share * final_score;
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_long_strong_beats_and_long_ending() {
        // Half note on 1, 8ths on 3-and, then a half note on 1 of bar 2.
        let f = fixture(
            "C",
            "C",
            2,
            &[(C4, 8), (D4, 2), (E4, 2), (F4, 2), (G4, 2), (E4, 8), (C4, 8)],
        );
        let r = LongNotesOnStrongBeats.score(&f.ctx(), f.rule_cfg("long_notes_on_strong_beats"));
        assert!(r.score > 0.5, "{}", r.score);
        assert!(!r.broken);
    }

    #[test]
    fn fails_with_short_ending_and_long_offbeats() {
        // 8th on 1, dotted quarter on the "and", 8th on 3, dotted quarter, 8th end.
        let f = fixture("C", "C", 1, &[(C4, 2), (D4, 6), (E4, 2), (G4, 4), (E4, 2)]);
        let r = LongNotesOnStrongBeats.score(&f.ctx(), f.rule_cfg("long_notes_on_strong_beats"));
        assert!(r.score < 0.0, "{}", r.score);
        assert!(r.broken);
        assert!(r.details.iter().any(|d| d.text.contains("final note")));
    }
}
