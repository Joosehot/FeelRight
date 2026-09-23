//! Rule 37: semitone clashes. A non-chord tone a half step from a chord
//! tone (a minor 2nd or minor 9th against the accompaniment) is harsh
//! when it is held or on a strong beat and does not resolve by step.
//! Cross-relations (D against D#) fall under the same check.

use super::analysis::analyze;
use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::theory::PitchClass;

pub struct Clash;

impl Rule for Clash {
    fn name(&self) -> &'static str {
        "clash"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_dur"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let mut res = RuleResult::default();
        let mut hits = 0;
        let min_dur = cfg.param("min_dur") as u32;
        for info in analyze(ctx) {
            let Some(chord) = info.chord else { continue };
            let pc = PitchClass::of_midi(info.note.pitch);
            if chord.contains(pc) {
                continue;
            }
            let semitone = chord.tones.iter().any(|t| {
                let d = t.interval_to(pc);
                d == 1 || d == 11
            });
            if !semitone {
                continue;
            }
            let exposed = info.strong || info.note.dur >= min_dur;
            let resolved = info.next_iv.map(|b| matches!(b.abs(), 1 | 2)).unwrap_or(false);
            if exposed && !resolved {
                hits += 1;
                res.broken = true;
                res.tension += cfg.tension;
                res.details.push(Detail {
                    bar: info.bar,
                    score: -1.0,
                    text: format!("{pc} clashes by a semitone with {chord}"),
                });
            } else if exposed {
                // Resolved but exposed: mild.
                hits += 0;
                res.details.push(Detail {
                    bar: info.bar,
                    score: -0.25,
                    text: format!("{pc} leans on {chord} (resolved)"),
                });
            }
        }
        let mild: f32 = res.details.iter().filter(|d| d.score == -0.25).count() as f32 * 0.25;
        res.score = (1.0 - 2.0 * hits as f32 - mild).max(-1.0);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;

    #[test]
    fn passes_with_chord_tones_and_passing_tones() {
        let f = fixture("C", "C", 1, &[(C4, 4), (D4, 2), (E4, 2), (F4, 2), (G4, 6)]);
        let r = Clash.score(&f.ctx(), f.rule_cfg("clash"));
        assert!(r.score >= 0.75, "{}", r.score);
        assert!(!r.broken);
    }

    #[test]
    fn fails_with_held_semitone_against_chord() {
        // D natural held over B7 (which has D#), leaping away.
        let f = fixture("B7", "E:minor", 1, &[(D4, 8), (B4, 8)]);
        let r = Clash.score(&f.ctx(), f.rule_cfg("clash"));
        assert!(r.score < 0.0, "{}", r.score);
        assert!(r.broken);
    }
}
