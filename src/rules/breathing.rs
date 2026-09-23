//! Rule 24: a rest or a long note at the end of every phrase, and a
//! shorter breath at the end of every 2-bar sub-phrase.

use super::{Context, Detail, Rule, RuleResult};
use crate::config::RuleConfig;
use crate::model::Event;

pub struct Breathing;

impl Rule for Breathing {
    fn name(&self) -> &'static str {
        "breathing"
    }
    fn params(&self) -> &'static [&'static str] {
        &["min_dur", "sub_min_dur"]
    }

    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult {
        let present = ctx.bars_present();
        let min = cfg.param("min_dur") as u32;
        let mut res = RuleResult::default();
        let mut sum = 0.0;
        let mut n = 0;
        let sub_min = cfg.param("sub_min_dur") as u32;
        let phrase_ends = ctx.form.phrase_ends();
        // Sub-phrase ends: every second bar counted from each phrase start.
        let mut sub_ends = Vec::new();
        let mut start = 0;
        for &end in &phrase_ends {
            let mut b = start + 1;
            while b < end {
                sub_ends.push(b);
                b += 2;
            }
            start = end + 1;
        }
        for (end, need, label) in phrase_ends
            .iter()
            .map(|&e| (e, min, "phrase ends without a breath"))
            .chain(sub_ends.iter().map(|&e| (e, sub_min, "sub-phrase ends without a breath")))
        {
            if end >= present {
                continue;
            }
            let Some(last) = ctx.bar_events(end).last().copied() else { continue };
            n += 1;
            let ok = match last {
                Event::Rest { .. } => true,
                Event::Note(nt) => nt.dur >= need,
            };
            if ok {
                sum += 1.0;
            } else {
                sum -= 1.0;
                res.broken = true;
                res.tension += cfg.tension;
                res.details.push(Detail { bar: end, score: -1.0, text: label.into() });
            }
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
    fn passes_with_long_phrase_endings() {
        let f = fixture("C G C G C G C C", "C", 8, &[(C4, 16), (D4, 16), (E4, 16), (D4, 8), (D4, 8), (C4, 16), (D4, 16), (E4, 16), (C4, 16)]);
        assert_eq!(Breathing.score(&f.ctx(), f.rule_cfg("breathing")).score, 1.0);
    }

    #[test]
    fn fails_with_busy_phrase_endings() {
        let f = fixture("C G C G C G C C", "C", 8, &[(C4, 16), (D4, 16), (E4, 16), (D4, 12), (E4, 2), (D4, 2), (C4, 16), (D4, 16), (E4, 16), (C4, 12), (D4, 2), (C4, 2)]);
        let r = Breathing.score(&f.ctx(), f.rule_cfg("breathing"));
        assert!(r.score <= 0.0, "{}", r.score);
        assert!(r.broken);
        assert_eq!(r.details.len(), 2);
    }
}
