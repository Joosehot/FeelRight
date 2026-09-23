//! Rule 3: the natural 4th held on a strong beat over a major triad, and
//! the b9 over a dominant 7th, are penalized unless they resolve by step.

use super::analysis::analyze;
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::Quality;

pub struct AvoidNotes;

impl Rule for AvoidNotes {
    fn name(&self) -> &'static str {
        "avoid_notes"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_dur"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let mut hits = 0;
        for info in analyze(ctx) {
            let Some(chord) = info.chord else { continue };
            let iv = chord.root.interval_to(crate::theory::PitchClass::of_midi(info.note.pitch));
            let avoid = match chord.quality {
                Quality::Maj | Quality::Maj7 => iv == 5 && info.strong && info.note.dur as f32 >= cfg.param("min_dur"),
                Quality::Dom7 => iv == 1,
                _ => false,
            };
            if !avoid {
                continue;
            }
            let resolved = info.next_iv.map(|b| matches!(b.abs(), 1 | 2)).unwrap_or(false);
            if resolved {
                continue;
            }
            hits += 1;
            res.broken = true;
            res.tension += cfg.tension;
            res.details.push(Detail {
                bar: info.bar,
                score: -1.0,
                text: format!("avoid note {} over {chord} left unresolved", crate::theory::PitchClass::of_midi(info.note.pitch)),
            });
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
    fn passes_when_fourth_resolves() {
        let f = fixture("C", "C", 1, &[(F4, 4), (E4, 4), (G4, 8)]);
        let r = AvoidNotes.score(&f.ctx(), f.rule_cfg("avoid_notes"));
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn fails_with_held_fourth_over_major() {
        let f = fixture("C", "C", 1, &[(F4, 8), (C5, 8)]);
        let r = AvoidNotes.score(&f.ctx(), f.rule_cfg("avoid_notes"));
        assert_eq!(r.score, -1.0);
        assert!(r.broken);
    }

    #[test]
    fn fails_with_flat_nine_over_dominant() {
        // Ab over G7, leaping away.
        let f = fixture("G7", "C", 1, &[(G4, 4), (68, 4), (D5, 8)]);
        let r = AvoidNotes.score(&f.ctx(), f.rule_cfg("avoid_notes"));
        assert!(r.broken);
    }
}
