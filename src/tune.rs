//! M7: fit rule weights to taste.
//!
//! Positive examples are real melodies (a corpus of public-domain tunes
//! given as fixed-melody suites) and the preferred side of A/B ratings.
//! Negatives are random chord-tone melodies and corrupted copies of the
//! positives over the same chords, plus the rejected side of ratings.
//! A hill-climbing random search scales rule weights so that positives
//! outscore negatives on as many pairs as possible. 20 % of the pairs are
//! held out and reported, never optimized.

use crate::config::Config;
use crate::form::{Form, Template};
use crate::generate::random_chord_tone_melody;
use crate::model::{Event, Melody, Meter, Note, Style};
use crate::parser::{self, ChordSpan};
use crate::rules::{self, Rule};
use crate::search::{beam_search, SearchInput};
use crate::suite::{self, SuiteFile};
use crate::tension;
use crate::theory::Key;
use anyhow::{Context as _, Result};
use clap::ValueEnum;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One melody with everything needed to score it.
pub struct Example {
    pub label: String,
    pub melody: Melody,
    pub chords: Vec<ChordSpan>,
    pub key: Key,
    pub meter: Meter,
    pub bars: u32,
    pub tension: Vec<f32>,
    pub form: Form,
}

impl Example {
    pub fn score(&self, cfg: &Config, rules: &[Box<dyn Rule>]) -> f32 {
        let ctx = rules::Context {
            melody: &self.melody,
            chords: &self.chords,
            key: self.key,
            meter: self.meter,
            bars: self.bars,
            tension: &self.tension,
            complete: true,
            form: &self.form,
            ends_open: false,
        };
        rules::evaluate(&ctx, cfg, rules).total
    }
}

/// A pair where `good` should outscore `bad`.
pub struct Pair {
    pub good: usize,
    pub bad: usize,
    pub source: String,
}

/// One line of ratings.jsonl, written by `melody rate`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Rating {
    pub chords: String,
    pub key: String,
    #[serde(default = "default_meter")]
    pub meter: String,
    pub bars: u32,
    #[serde(default = "default_style")]
    pub style: String,
    #[serde(default = "default_form")]
    pub form: String,
    #[serde(default)]
    pub tension: Option<Vec<f32>>,
    pub seed_a: u64,
    pub seed_b: u64,
    /// "a" or "b".
    pub preferred: String,
}

fn default_meter() -> String {
    "4/4".into()
}
fn default_style() -> String {
    "classical".into()
}
fn default_form() -> String {
    "auto".into()
}

fn corrupt(m: &Melody, rng: &mut ChaCha8Rng, share: f32) -> Melody {
    let mut out = m.clone();
    for e in &mut out.events {
        if let Event::Note(n) = e {
            if rng.gen::<f32>() < share {
                let d: i32 = *[-3, -2, -1, 1, 2, 3].choose(rng).unwrap();
                n.pitch = (n.pitch as i32 + d).clamp(48, 96) as u8;
            }
        }
    }
    out
}

/// Bars shuffled: same material, no form.
fn shuffle_bars(m: &Melody, meter: Meter, bars: u32, rng: &mut ChaCha8Rng) -> Melody {
    let spb = meter.steps_per_bar();
    let mut order: Vec<u32> = (0..bars).collect();
    order.shuffle(rng);
    let mut out = Melody::default();
    for (slot, &src) in order.iter().enumerate() {
        for e in m.bar_events(src, meter) {
            let shift = slot as i64 * spb as i64 - (src * spb) as i64;
            out.push(match e {
                Event::Note(n) => Event::Note(Note { start: (n.start as i64 + shift) as u32, ..n }),
                Event::Rest { start, dur } => Event::Rest { start: (start as i64 + shift) as u32, dur },
            });
        }
    }
    out.events.sort_by_key(|e| e.start());
    out
}

pub struct Dataset {
    pub examples: Vec<Example>,
    pub pairs: Vec<Pair>,
}

/// Build the dataset from corpus suites and ratings.
pub fn build(corpus: &[SuiteFile], ratings: &[Rating], negatives_per_positive: u32, cfg: &Config, rules: &[Box<dyn Rule>]) -> Result<Dataset> {
    let mut examples = Vec::new();
    let mut pairs = Vec::new();
    let mut rng = ChaCha8Rng::seed_from_u64(7);

    for file in corpus {
        for sec in &file.section {
            let Some(text) = &sec.melody else { continue };
            let key = parser::parse_key(&sec.key)?;
            let meter = parser::parse_meter(&sec.meter)?;
            let chords = parser::parse_progression(&sec.chords, meter, sec.bars)?;
            let melody = parser::parse_melody(text, sec.transpose)?;
            let template = Template::from_str(&sec.form, true).map_err(|e| anyhow::anyhow!(e))?;
            let form = Form::plan(template, sec.bars);
            let tension = sec.tension.clone().unwrap_or_else(|| tension::default_curve(sec.bars));
            let label = format!("{} / {}", file.name, sec.name);
            let good = examples.len();
            examples.push(Example { label: label.clone(), melody: melody.clone(), chords: chords.clone(), key, meter, bars: sec.bars, tension: tension.clone(), form: form.clone() });
            let style = Style::from_str(&sec.style, true).unwrap_or(Style::Classical);
            for k in 0..negatives_per_positive {
                let (neg, kind) = match k % 3 {
                    0 => (random_chord_tone_melody(&chords, meter, sec.bars, 100 + k as u64, style), "random"),
                    1 => (corrupt(&melody, &mut rng, 0.35), "corrupted"),
                    _ => (shuffle_bars(&melody, meter, sec.bars, &mut rng), "shuffled"),
                };
                let bad = examples.len();
                examples.push(Example { label: format!("{label} [{kind} {k}]"), melody: neg, chords: chords.clone(), key, meter, bars: sec.bars, tension: tension.clone(), form: form.clone() });
                pairs.push(Pair { good, bad, source: format!("corpus:{kind}") });
            }
        }
    }

    for (i, r) in ratings.iter().enumerate() {
        let key = parser::parse_key(&r.key)?;
        let meter = parser::parse_meter(&r.meter)?;
        let chords = parser::parse_progression(&r.chords, meter, r.bars)?;
        let style = Style::from_str(&r.style, true).map_err(|e| anyhow::anyhow!(e))?;
        let template = Template::from_str(&r.form, true).map_err(|e| anyhow::anyhow!(e))?;
        let form = Form::plan(template, r.bars);
        let tension = r.tension.clone().unwrap_or_else(|| tension::default_curve(r.bars));
        let mut gen = |seed: u64| -> Melody {
            let input = SearchInput { chords: &chords, key, meter, bars: r.bars, tension: &tension, style, seed, form: &form, ends_open: false, theme: None };
            beam_search(&input, cfg, rules).0
        };
        let a = gen(r.seed_a);
        let b = gen(r.seed_b);
        let (good_m, bad_m) = if r.preferred == "a" { (a, b) } else { (b, a) };
        let good = examples.len();
        examples.push(Example { label: format!("rating {i} preferred"), melody: good_m, chords: chords.clone(), key, meter, bars: r.bars, tension: tension.clone(), form: form.clone() });
        let bad = examples.len();
        examples.push(Example { label: format!("rating {i} rejected"), melody: bad_m, chords, key, meter, bars: r.bars, tension, form });
        pairs.push(Pair { good, bad, source: "rating".into() });
    }
    Ok(Dataset { examples, pairs })
}

/// Fraction of pairs where good outscores bad, plus a small margin term
/// so ties and near-misses still give gradient.
fn objective(scores: &[f32], pairs: &[&Pair]) -> (f32, f32) {
    if pairs.is_empty() {
        return (0.0, 0.0);
    }
    let mut correct = 0.0;
    let mut margin = 0.0;
    for p in pairs {
        let d = scores[p.good] - scores[p.bad];
        if d > 0.0 {
            correct += 1.0;
        }
        margin += (d / 10.0).clamp(-1.0, 1.0);
    }
    let n = pairs.len() as f32;
    (correct / n, correct / n + 0.05 * margin / n)
}

pub struct TuneReport {
    pub before_train: f32,
    pub before_holdout: f32,
    pub after_train: f32,
    pub after_holdout: f32,
    pub accepted: u32,
    pub changes: Vec<(String, f32, f32)>,
}

pub fn tune(data: &Dataset, base: &Config, rules: &[Box<dyn Rule>], iters: u32, seed: u64) -> (Config, TuneReport) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    // 80/20 split, deterministic.
    let mut idx: Vec<usize> = (0..data.pairs.len()).collect();
    idx.shuffle(&mut rng);
    let cut = (idx.len() * 4) / 5;
    let train: Vec<&Pair> = idx[..cut].iter().map(|&i| &data.pairs[i]).collect();
    let hold: Vec<&Pair> = idx[cut..].iter().map(|&i| &data.pairs[i]).collect();

    let score_all = |cfg: &Config| -> Vec<f32> { data.examples.iter().map(|e| e.score(cfg, rules)).collect() };
    let names: Vec<String> = rules.iter().map(|r| r.name().to_string()).collect();

    let mut best = base.clone();
    let mut scores = score_all(&best);
    let (before_train, mut best_obj) = objective(&scores, &train);
    let (before_holdout, _) = objective(&scores, &hold);
    let mut accepted = 0;

    for it in 0..iters {
        let mut cand = best.clone();
        // Perturb one to three weights; occasionally the tension lambda.
        let n = 1 + (it % 3) as usize;
        for _ in 0..n {
            let name = names.choose(&mut rng).unwrap();
            let rc = cand.rules.get_mut(name).unwrap();
            let factor = (rng.gen::<f32>() * 1.2 - 0.6).exp();
            rc.weight = (rc.weight * factor).clamp(0.0, 12.0);
        }
        if rng.gen::<f32>() < 0.15 {
            let factor = (rng.gen::<f32>() * 1.0 - 0.5).exp();
            cand.search.tension_lambda = (cand.search.tension_lambda * factor).clamp(0.0, 20.0);
        }
        let s = score_all(&cand);
        let (_, obj) = objective(&s, &train);
        if obj >= best_obj {
            if obj > best_obj {
                accepted += 1;
            }
            best_obj = obj;
            best = cand;
            scores = s;
        }
    }
    let (after_train, _) = objective(&scores, &train);
    let (after_holdout, _) = objective(&scores, &hold);
    let mut changes = Vec::new();
    for name in &names {
        let (a, b) = (base.rules[name].weight, best.rules[name].weight);
        if (a - b).abs() > 0.05 {
            changes.push((name.clone(), a, b));
        }
    }
    if (base.search.tension_lambda - best.search.tension_lambda).abs() > 0.05 {
        changes.push(("search.tension_lambda".into(), base.search.tension_lambda, best.search.tension_lambda));
    }
    (best, TuneReport { before_train, before_holdout, after_train, after_holdout, accepted, changes })
}

/// Write the tuned weights into a copy of a rules.toml text, keeping
/// comments and parameters.
pub fn write_tuned(template_text: &str, cfg: &Config, out: &Path) -> Result<()> {
    let mut text = template_text.to_string();
    for (name, rc) in &cfg.rules {
        let header = format!("[rules.{name}]");
        if let Some(h) = text.find(&header) {
            let block_start = h + header.len();
            let rest = &text[block_start..];
            if let Some(w) = rest.find("weight = ") {
                let line_end = rest[w..].find('\n').map(|e| w + e).unwrap_or(rest.len());
                let abs_start = block_start + w;
                let abs_end = block_start + line_end;
                text.replace_range(abs_start..abs_end, &format!("weight = {:.2}", rc.weight));
            }
        }
    }
    if let Some(l) = text.find("tension_lambda = ") {
        let end = text[l..].find('\n').map(|e| l + e).unwrap_or(text.len());
        let comment = text[l..end].find('#').map(|c| text[l + c..end].to_string()).unwrap_or_default();
        text.replace_range(l..end, &format!("tension_lambda = {:.2}      {comment}", cfg.search.tension_lambda));
    }
    std::fs::write(out, text).with_context(|| format!("writing {}", out.display()))
}

pub fn load_corpus(dir: &Path) -> Result<Vec<SuiteFile>> {
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading corpus dir {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "toml").unwrap_or(false))
        .collect();
    entries.sort();
    for p in entries {
        out.push(suite::load(&p)?);
    }
    Ok(out)
}

pub fn load_ratings(path: &Path) -> Result<Vec<Rating>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(serde_json::from_str(line).with_context(|| format!("ratings line {}", i + 1))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::all_rules;

    #[test]
    fn corrupt_and_shuffle_keep_length() {
        let meter = Meter { num: 4, den: 4 };
        let m = parser::parse_melody("C4:4 D4:4 E4:8 | G4:8 E4:8", 0).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        assert_eq!(corrupt(&m, &mut rng, 1.0).total_steps(), 32);
        let s = shuffle_bars(&m, meter, 2, &mut rng);
        assert_eq!(s.total_steps(), 32);
        assert_eq!(s.notes().count(), 5);
    }

    #[test]
    fn tuning_does_not_lower_train_accuracy() {
        let cfg = Config::default_config();
        let rules = all_rules();
        let corpus: SuiteFile = toml::from_str(
            r#"
name = "t"
[[section]]
name = "s"
chords = "C G Am F"
key = "C:major"
tempo = 100
bars = 4
melody = "E4:4 D4:4 C4:8 | D4:4 E4:4 D4:8 | C4:4 E4:4 G4:8 | A4:4 G4:4 F4:4 E4:4"
"#,
        )
        .unwrap();
        let data = build(&[corpus], &[], 6, &cfg, &rules).unwrap();
        assert_eq!(data.pairs.len(), 6);
        let (_, rep) = tune(&data, &cfg, &rules, 30, 1);
        assert!(rep.after_train >= rep.before_train);
    }
}
