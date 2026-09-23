//! The rule system. Every rule lives in its own file, implements `Rule`,
//! and reads its numbers from `rules.toml` via `RuleConfig`.

use crate::config::{Config, RuleConfig};
use crate::form::Form;
use crate::model::{chord_at, Event, Melody, Meter, Note};
use crate::parser::ChordSpan;
use crate::theory::{Chord, Key};

pub mod analysis;
pub mod arrival;
pub mod avoid_notes;
pub mod awkward_intervals;
pub mod balanced_direction;
pub mod breathing;
pub mod callback;
pub mod chord_tones_on_strong_beats;
pub mod clash;
pub mod consecutive_leaps;
pub mod delayed_resolution;
pub mod density;
pub mod diatonic;
pub mod final_note;
pub mod gap_fill;
pub mod guide_tones;
pub mod leading_tone;
pub mod long_notes_on_strong_beats;
pub mod max_leap;
pub mod monotone;
pub mod mostly_steps;
pub mod nct_types;
pub mod noodling;
pub mod phrase_arch;
pub mod question_answer;
pub mod range;
pub mod repetition_with_variation;
pub mod resolve_after_peak;
pub mod rhythmic_motif;
pub mod rhythmic_variety;
pub mod runs;
pub mod sequence;
pub mod seventh_resolves;
pub mod single_climax;
pub mod smooth_changes;
pub mod surprise;
pub mod syncopation;

/// Everything a rule may look at.
pub struct Context<'a> {
    pub melody: &'a Melody,
    pub chords: &'a [ChordSpan],
    pub key: Key,
    pub meter: Meter,
    pub bars: u32,
    /// Per-bar tension targets (empty until M5).
    pub tension: &'a [f32],
    /// False while the search is scoring a prefix; rules about the ending
    /// stay neutral until the melody is complete.
    pub complete: bool,
    /// Phrase plan (bar roles).
    pub form: &'a Form,
    /// The melody leads into another section: ending rules expect an
    /// open ending (degree 2, 5 or 7) instead of the tonic.
    pub ends_open: bool,
}

impl<'a> Context<'a> {
    pub fn notes(&self) -> Vec<&'a Note> {
        self.melody.notes().collect()
    }
    pub fn pos_in_bar(&self, step: u32) -> u32 {
        step % self.meter.steps_per_bar()
    }
    pub fn bar_of(&self, step: u32) -> u32 {
        step / self.meter.steps_per_bar()
    }
    pub fn strength(&self, step: u32) -> f32 {
        self.meter.strength(self.pos_in_bar(step))
    }
    pub fn is_strong(&self, step: u32) -> bool {
        self.meter.is_strong(self.pos_in_bar(step))
    }
    pub fn chord_at(&self, step: u32) -> Option<&'a Chord> {
        chord_at(self.chords, step)
    }
    /// Target tension for a bar (0.5 when no curve is given).
    pub fn target(&self, bar: u32) -> f32 {
        self.tension.get(bar as usize).copied().unwrap_or(0.5)
    }
    pub fn bar_events(&self, bar: u32) -> Vec<Event> {
        self.melody.bar_events(bar, self.meter)
    }
    /// Number of bars present (fully or partly) in the melody.
    pub fn bars_present(&self) -> u32 {
        let total = self.melody.total_steps();
        let spb = self.meter.steps_per_bar();
        (total + spb - 1) / spb
    }
    /// Signed intervals in semitones between consecutive notes.
    pub fn intervals(&self) -> Vec<i32> {
        self.notes()
            .windows(2)
            .map(|w| w[1].pitch as i32 - w[0].pitch as i32)
            .collect()
    }
}

/// One line of explanation, attached to a bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub bar: u32,
    pub score: f32,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleResult {
    /// Normalized score, roughly -1..=1. Multiplied by the rule weight.
    pub score: f32,
    /// The rule was violated somewhere (used with `breakable`/`tension`).
    pub broken: bool,
    /// Tension this rule adds, summed over bars where it fired.
    pub tension: f32,
    /// Candidate must be discarded (hard constraint).
    pub hard_violation: bool,
    /// Per-bar notes for `--explain`.
    pub details: Vec<Detail>,
}

impl RuleResult {
    pub fn score(score: f32) -> Self {
        RuleResult { score, ..Default::default() }
    }
}

pub trait Rule: Send + Sync {
    /// Key in `rules.toml`.
    fn name(&self) -> &'static str;
    /// Required numeric parameters (besides weight/breakable/tension).
    fn params(&self) -> &'static [&'static str] {
        &[]
    }
    fn score(&self, ctx: &Context, cfg: &RuleConfig) -> RuleResult;
}

/// Every rule, in numbering order of the spec.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        // Harmony
        Box::new(chord_tones_on_strong_beats::ChordTonesOnStrongBeats), // 1
        Box::new(nct_types::NctTypes),                                 // 2
        Box::new(avoid_notes::AvoidNotes),                             // 3
        Box::new(diatonic::Diatonic),                                  // 4
        Box::new(leading_tone::LeadingTone),                           // 5
        Box::new(seventh_resolves::SeventhResolves),                   // 6
        Box::new(guide_tones::GuideTones),                             // 7
        Box::new(smooth_changes::SmoothChanges),                       // 8
        // Contour and motion
        Box::new(mostly_steps::MostlySteps),                           // 9
        Box::new(gap_fill::GapFill),                                   // 10
        Box::new(consecutive_leaps::ConsecutiveLeaps),                 // 11
        Box::new(awkward_intervals::AwkwardIntervals),                 // 12
        Box::new(max_leap::MaxLeap),                                   // 13
        Box::new(single_climax::SingleClimax),                         // 14
        Box::new(range::Range),                                        // 15
        Box::new(monotone::Monotone),                                  // 16
        Box::new(noodling::Noodling),                                  // 17
        Box::new(runs::Runs),                                          // 18
        Box::new(balanced_direction::BalancedDirection),               // 19
        // Rhythm
        Box::new(rhythmic_motif::RhythmicMotif),                       // 20
        Box::new(rhythmic_variety::RhythmicVariety),                   // 21
        Box::new(long_notes_on_strong_beats::LongNotesOnStrongBeats),  // 22
        Box::new(syncopation::Syncopation),                            // 23
        Box::new(breathing::Breathing),                                // 24
        Box::new(density::Density),                                    // 25
        // Form and motif (26 is the planner in form.rs)
        Box::new(repetition_with_variation::RepetitionWithVariation),  // 27
        Box::new(sequence::Sequence),                                  // 28
        Box::new(question_answer::QuestionAnswer),                     // 29
        Box::new(final_note::FinalNote),                               // 30
        Box::new(callback::Callback),                                  // 31
        // Expectation and feel
        Box::new(surprise::Surprise),                                  // 32
        Box::new(delayed_resolution::DelayedResolution),               // 33
        Box::new(resolve_after_peak::ResolveAfterPeak),                // 34
        Box::new(arrival::Arrival),                                    // 35
        // Phrasing
        Box::new(phrase_arch::PhraseArch),                             // 36
        // Dissonance control
        Box::new(clash::Clash),                                        // 37
    ]
}

#[derive(Clone, Debug)]
pub struct RuleScore {
    pub name: &'static str,
    pub weight: f32,
    pub breakable: bool,
    pub result: RuleResult,
    /// Score after breakable-rule forgiveness (equals result.score when
    /// nothing was forgiven).
    pub adjusted: f32,
    /// How much penalty was forgiven because the target tension was high.
    pub forgiven: f32,
}

impl RuleScore {
    pub fn weighted(&self) -> f32 {
        self.weight * self.adjusted
    }
}

#[derive(Clone, Debug)]
pub struct Evaluation {
    pub total: f32,
    pub hard_violation: bool,
    pub rules: Vec<RuleScore>,
    /// Observed tension per bar present.
    pub observed: Vec<f32>,
    /// `-lambda * sum (observed - target)^2`.
    pub tension_term: f32,
}

/// Score a melody: `Σ weight × score` (with breakable-rule forgiveness in
/// high-tension bars) plus the tension-match term.
pub fn evaluate(ctx: &Context, cfg: &Config, rules: &[Box<dyn Rule>]) -> Evaluation {
    let mut out = Evaluation { total: 0.0, hard_violation: false, rules: Vec::new(), observed: vec![], tension_term: 0.0 };
    // (bar, tension) for every broken breakable rule detail.
    let mut breakable_tension: Vec<(u32, f32)> = Vec::new();
    let discount = cfg.search.breakable_discount;
    for r in rules {
        let rc = cfg.rule(r.name()).expect("config validated at load");
        let result = r.score(ctx, rc);
        out.hard_violation |= result.hard_violation;
        let mut adjusted = result.score;
        let mut forgiven = 0.0;
        if rc.breakable && result.broken {
            let bars: Vec<u32> = result.details.iter().filter(|d| d.score < 0.0).map(|d| d.bar).collect();
            for &b in &bars {
                breakable_tension.push((b, rc.tension));
            }
            if !bars.is_empty() && discount > 0.0 && result.score < 1.0 {
                let t = bars.iter().map(|&b| ctx.target(b)).sum::<f32>() / bars.len() as f32;
                forgiven = (1.0 - result.score) * t * discount;
                adjusted = result.score + forgiven;
            }
        }
        let rs = RuleScore { name: r.name(), weight: rc.weight, breakable: rc.breakable, result, adjusted, forgiven };
        out.total += rs.weighted();
        out.rules.push(rs);
    }
    out.observed = crate::tension::observed(ctx, &out.rules, &breakable_tension, &cfg.tension);
    out.tension_term = crate::tension::match_term(&out.observed, ctx.tension, cfg.search.tension_lambda);
    out.total += out.tension_term;
    out
}

/// Shared helpers for rule tests.
#[cfg(test)]
pub mod test_util {
    use super::*;
    use crate::config::Config;
    use crate::model::{Event, Melody, Meter, Note};
    use crate::parser::{parse_key, parse_progression};

    pub struct Fixture {
        pub melody: Melody,
        pub chords: Vec<ChordSpan>,
        pub key: Key,
        pub meter: Meter,
        pub bars: u32,
        pub cfg: Config,
        pub form: Form,
    }

    impl Fixture {
        pub fn ctx(&self) -> Context<'_> {
            Context {
                melody: &self.melody,
                chords: &self.chords,
                key: self.key,
                meter: self.meter,
                bars: self.bars,
                tension: &[],
                complete: true,
                form: &self.form,
                ends_open: false,
            }
        }
        pub fn rule_cfg(&self, name: &str) -> &RuleConfig {
            self.cfg.rule(name).unwrap()
        }
    }

    /// Build a fixture from `(pitch, dur)` pairs laid end to end. Pitch 0
    /// means a rest.
    pub fn fixture(chords: &str, key: &str, bars: u32, notes: &[(u8, u32)]) -> Fixture {
        let meter = Meter { num: 4, den: 4 };
        let chords = parse_progression(chords, meter, bars).unwrap();
        let key = parse_key(key).unwrap();
        let mut melody = Melody::default();
        let mut t = 0;
        for &(pitch, dur) in notes {
            if pitch == 0 {
                melody.push(Event::Rest { start: t, dur });
            } else {
                melody.push(Event::Note(Note { pitch, start: t, dur }));
            }
            t += dur;
        }
        let form = Form::plan(crate::form::Template::Auto, bars);
        Fixture { melody, chords, key, meter, bars, cfg: Config::default_config(), form }
    }

    // MIDI pitch names for readable tests.
    pub const C4: u8 = 60;
    pub const D4: u8 = 62;
    pub const E4: u8 = 64;
    pub const F4: u8 = 65;
    pub const G4: u8 = 67;
    pub const A4: u8 = 69;
    pub const B4: u8 = 71;
    pub const C5: u8 = 72;
    pub const D5: u8 = 74;
    pub const E5: u8 = 76;
    pub const F5: u8 = 77;
    pub const G5: u8 = 79;
    pub const A5: u8 = 81;
}
