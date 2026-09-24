//! Multi-section pieces: the suite file format, rendering, and a seed
//! search per section driven by the evaluator.

use crate::config::Config;
use crate::form::{Form, Template};
use crate::model::{Melody, Meter, Style};
use crate::parser::{self, ChordSpan};
use crate::rules::{self, Evaluation, Rule};
use crate::search::{self, SearchInput, Theme};
use crate::theory::{Chord, Key, Quality};
use std::collections::HashMap;
use crate::{midi, tension};
use anyhow::{bail, Context as _, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuiteFile {
    #[serde(default)]
    pub name: String,
    pub section: Vec<SuiteSection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SuiteSection {
    #[serde(default)]
    pub name: String,
    pub chords: String,
    pub key: String,
    #[serde(default = "default_meter")]
    pub meter: String,
    pub tempo: u32,
    pub bars: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tension: Option<Vec<f32>>,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "default_instrument")]
    pub instrument: String,
    #[serde(default)]
    pub octave: i32,
    #[serde(default = "default_style")]
    pub style: String,
    #[serde(default = "default_form")]
    pub form: String,
    /// Fixed melody in `C4:4 D4:2 r:2` notation; skips the search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub melody: Option<String>,
    /// Semitone shift applied to a fixed melody.
    #[serde(default)]
    pub transpose: i32,
    /// The section leads into the next one: ending rules expect an open
    /// (dominant-side) ending instead of the tonic.
    #[serde(default)]
    pub ends_open: bool,
    /// Borrow the opening motif of the named section (transformed to this
    /// section's key and harmony).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_from: Option<String>,
    /// Extra bars appended on the dominant of the next section's key,
    /// leading into it. Forces an open ending.
    #[serde(default)]
    pub bridge: u32,
    /// Per-section rule overrides, e.g. `max_leap = { soft_max = 12 }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_override: Option<toml::Table>,
    /// Refinement rounds after the search: each round rewrites one bar
    /// and keeps it only if the score improves. 0 = off.
    #[serde(default)]
    pub refine: u32,
    /// Instrument for the accompaniment (chords and bass) instead of the
    /// style's default, e.g. "harp" for a solo harp piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accompaniment: Option<String>,
}

fn default_meter() -> String {
    "4/4".into()
}
fn default_seed() -> u64 {
    1
}
fn default_instrument() -> String {
    "piano".into()
}
fn default_style() -> String {
    "orchestral".into()
}
fn default_form() -> String {
    "auto".into()
}

pub fn load(path: &Path) -> Result<SuiteFile> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).context("parsing suite file")
}

pub fn save(path: &Path, file: &SuiteFile) -> Result<()> {
    let text = toml::to_string_pretty(file).context("serializing suite")?;
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// General MIDI program for a few common instrument names, or a number.
pub fn gm_program(name: &str) -> Result<u8> {
    let n = name.trim().to_ascii_lowercase();
    Ok(match n.as_str() {
        "piano" => 0,
        "harpsichord" => 6,
        "organ" | "church_organ" => 19,
        "harp" => 46,
        "music_box" => 10,
        "lead" | "square_lead" => 80,
        "saw_lead" => 81,
        "pad" => 89,
        "808" => 39,
        "celesta" => 8,
        "harmonica" => 22,
        "steel_guitar" => 25,
        "glockenspiel" => 9,
        "guitar" => 24,
        "violin" => 40,
        "viola" => 41,
        "cello" => 42,
        "strings" => 48,
        "timpani" => 47,
        "trumpet" => 56,
        "trombone" => 57,
        "tuba" => 58,
        "muted_trumpet" => 59,
        "horn" => 60,
        "brass" => 61,
        "oboe" => 68,
        "clarinet" => 71,
        "flute" => 73,
        "voice" | "choir" | "tenor" => 52,
        "bassoon" => 70,
        "nylon_guitar" | "classical_guitar" => 24,
        "lute" => 24,
        "piccolo" => 72,
        "xylophone" => 13,
        "pizzicato" => 45,
        "accordion" => 21,
        "bandoneon" => 23,
        "sax" | "alto_sax" => 65,
        "tenor_sax" => 66,
        "epiano" => 4,
        "vibraphone" | "vibes" => 11,
        "jazz_guitar" => 26,
        "acoustic_bass" => 32,
        "oohs" => 53,
        "mandolin" => 25,
        _ => n.parse::<u8>().map_err(|_| anyhow::anyhow!("unknown instrument '{name}'"))?,
    })
}

pub struct Rendered {
    pub name: String,
    pub key: Key,
    pub melody: Melody,
    pub chords: Vec<ChordSpan>,
    pub meter: Meter,
    pub tempo: u32,
    pub style: Style,
    pub energy: Vec<f32>,
    pub program: u8,
    pub octave: i32,
    pub phrase_ends: Vec<u32>,
    pub eval: Evaluation,
    pub refine_log: Vec<search::RefineStep>,
    pub acc_program: Option<u8>,
}

/// What a section needs from its neighbours.
#[derive(Default)]
pub struct Neighbours<'a> {
    /// Key of the next section (for a bridge).
    pub next_key: Option<Key>,
    /// Borrowed opening motif.
    pub theme: Option<&'a Theme>,
}

/// Generate (or parse) one section's melody and score it.
pub fn render_section(sec: &SuiteSection, nb: &Neighbours, base_cfg: &Config, rules: &[Box<dyn Rule>]) -> Result<Rendered> {
    let cfg_owned;
    let cfg = match &sec.rules_override {
        Some(t) => {
            cfg_owned = base_cfg.with_overrides(t).with_context(|| format!("section '{}' rules_override", sec.name))?;
            &cfg_owned
        }
        None => base_cfg,
    };
    let key = parser::parse_key(&sec.key)?;
    let meter = parser::parse_meter(&sec.meter)?;
    let mut spans = parser::parse_progression(&sec.chords, meter, sec.bars)?;
    let mut tension = sec.tension.clone().unwrap_or_else(|| tension::default_curve(sec.bars));
    if tension.len() as u32 != sec.bars {
        bail!("section '{}': tension needs {} values, got {}", sec.name, sec.bars, tension.len());
    }
    let mut bars = sec.bars;
    let mut ends_open = sec.ends_open;
    if sec.bridge > 0 {
        let Some(next) = nb.next_key else {
            bail!("section '{}' has a bridge but no following section", sec.name);
        };
        // Dominant seventh of the next key, held for the bridge bars.
        let dom = Chord::new(next.tonic.add(7), Quality::Dom7);
        let spb = meter.steps_per_bar();
        for b in 0..sec.bridge {
            spans.push(ChordSpan { chord: dom.clone(), start: (bars + b) * spb, len: spb });
            let last = *tension.last().unwrap_or(&0.5);
            tension.push((last + 0.15).min(0.9));
        }
        bars += sec.bridge;
        ends_open = true;
    }
    let style = Style::from_str(&sec.style, true).map_err(|e| anyhow::anyhow!(e))?;
    let template = Template::from_str(&sec.form, true).map_err(|e| anyhow::anyhow!(e))?;
    let form = Form::plan(template, bars);
    let input = SearchInput {
        chords: &spans,
        key,
        meter,
        bars,
        tension: &tension,
        style,
        seed: sec.seed,
        form: &form,
        ends_open,
        theme: nb.theme,
    };
    let (melody, eval) = match &sec.melody {
        Some(text) => {
            let melody = parser::parse_melody(text, sec.transpose)?;
            let expected = bars * meter.steps_per_bar();
            if melody.total_steps() != expected {
                bail!(
                    "section '{}': fixed melody is {} steps, but {} bars need {}",
                    sec.name, melody.total_steps(), bars, expected
                );
            }
            let ctx = rules::Context {
                melody: &melody,
                chords: &spans,
                key,
                meter,
                bars,
                tension: &tension,
                complete: true,
                form: &form,
                ends_open,
            };
            let eval = rules::evaluate(&ctx, cfg, rules);
            (melody, eval)
        }
        None => search::beam_search(&input, cfg, rules),
    };
    let (melody, eval, refine_log) = if sec.refine > 0 && sec.melody.is_none() {
        search::refine(&input, cfg, rules, melody, sec.refine, (sec.refine / 3).max(5))
    } else {
        (melody, eval, vec![])
    };
    Ok(Rendered {
        name: sec.name.clone(),
        key,
        melody,
        chords: spans,
        meter,
        tempo: sec.tempo,
        style,
        energy: tension,
        program: gm_program(&sec.instrument)?,
        octave: sec.octave,
        phrase_ends: form.phrase_ends(),
        eval,
        refine_log,
        acc_program: match &sec.accompaniment {
            Some(name) => Some(gm_program(name)?),
            None => None,
        },
    })
}

fn next_key(file: &SuiteFile, i: usize) -> Result<Option<Key>> {
    match file.section.get(i + 1) {
        Some(n) => Ok(Some(parser::parse_key(&n.key)?)),
        None => Ok(None),
    }
}

fn theme_for<'a>(sec: &SuiteSection, themes: &'a HashMap<String, Theme>) -> Result<Option<&'a Theme>> {
    match &sec.theme_from {
        None => Ok(None),
        Some(name) => themes
            .get(name)
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("section '{}': theme_from '{name}' is not an earlier section", sec.name)),
    }
}

/// Render every section and write the MIDI. Returns the rendered
/// sections for reporting.
pub fn render(file: &SuiteFile, out: &Path, cfg: &Config, rules: &[Box<dyn Rule>]) -> Result<Vec<Rendered>> {
    let mut rendered: Vec<Rendered> = Vec::new();
    let mut themes: HashMap<String, Theme> = HashMap::new();
    for (i, sec) in file.section.iter().enumerate() {
        let nb = Neighbours { next_key: next_key(file, i)?, theme: theme_for(sec, &themes)? };
        let r = render_section(sec, &nb, cfg, rules)?;
        if let Some(t) = Theme::from_melody(&r.key, &r.melody, r.meter) {
            themes.insert(sec.name.clone(), t);
        }
        rendered.push(r);
    }
    let mut offset = 0;
    let mut sections = Vec::new();
    for r in &rendered {
        sections.push(midi::Section {
            name: &r.name,
            melody: &r.melody,
            chords: &r.chords,
            meter: r.meter,
            tempo_bpm: r.tempo,
            style: r.style,
            energy: &r.energy,
            program: r.program,
            octave: r.octave,
            phrase_ends: &r.phrase_ends,
            offset,
            acc_program: r.acc_program,
        });
        offset += r.melody.total_steps();
    }
    midi::write_suite(out, &sections)?;
    Ok(rendered)
}

pub fn total_bars(file: &SuiteFile) -> u32 {
    file.section.iter().map(|s| s.bars + s.bridge).sum()
}

/// For every generated section, try `seeds` seeds and keep the best by
/// evaluator score. Returns the improved suite and a log line per section.
/// Upper bound on seed batches when a target score is requested.
const MAX_TARGET_BATCHES: u32 = 12;

pub fn explore(file: &SuiteFile, seeds: u64, target: Option<f32>, cfg: &Config, rules: &[Box<dyn Rule>]) -> Result<(SuiteFile, Vec<String>)> {
    let mut best_file = file.clone();
    let mut log = Vec::new();
    let mut themes: HashMap<String, Theme> = HashMap::new();
    for (i, sec) in file.section.iter().enumerate() {
        let nb = Neighbours { next_key: next_key(file, i)?, theme: theme_for(sec, &themes)? };
        if sec.melody.is_some() {
            let r = render_section(sec, &nb, cfg, rules)?;
            if let Some(t) = Theme::from_melody(&r.key, &r.melody, r.meter) {
                themes.insert(sec.name.clone(), t);
            }
            continue;
        }
        // Render seeds in parallel batches; the search is deterministic
        // per seed, so the result does not depend on scheduling. With a
        // target score, keep adding batches until a seed reaches it.
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).max(1);
        let mut scored: Vec<(u64, Rendered)> = Vec::new();
        let mut batch = 0u32;
        loop {
            let first = sec.seed + batch as u64 * seeds;
            let seed_list: Vec<u64> = (0..seeds).map(|s| first + s).collect();
            let mut results: Vec<(u64, Result<Rendered>)> = Vec::with_capacity(seed_list.len());
            std::thread::scope(|scope| {
                let mut handles = Vec::new();
                for chunk in seed_list.chunks(seed_list.len().div_ceil(threads).max(1)) {
                    let chunk = chunk.to_vec();
                    let nb = &nb;
                    handles.push(scope.spawn(move || {
                        chunk
                            .into_iter()
                            .map(|seed| {
                                let mut trial = sec.clone();
                                trial.seed = seed;
                                (seed, render_section(&trial, nb, cfg, rules))
                            })
                            .collect::<Vec<_>>()
                    }));
                }
                for h in handles {
                    results.extend(h.join().expect("seed worker panicked"));
                }
            });
            for (seed, r) in results {
                scored.push((seed, r?));
            }
            scored.sort_by(|a, b| b.1.eval.total.partial_cmp(&a.1.eval.total).unwrap());
            batch += 1;
            let best = scored.first().map(|s| s.1.eval.total).unwrap_or(f32::NEG_INFINITY);
            match target {
                Some(t) if best < t && batch < MAX_TARGET_BATCHES => continue,
                _ => break,
            }
        }
        if let Some((seed, r)) = scored.first() {
            let (seed, score) = (*seed, r.eval.total);
            if let Some(t) = Theme::from_melody(&r.key, &r.melody, r.meter) {
                themes.insert(sec.name.clone(), t);
            }
            best_file.section[i].seed = seed;
            let label = if sec.name.is_empty() { format!("{}", i + 1) } else { sec.name.clone() };
            let worst = scored.last().map(|s| s.1.eval.total).unwrap_or(score);
            let top: Vec<String> = scored.iter().take(3).map(|(s, r)| format!("{s}={:.1}", r.eval.total)).collect();
            let reached = match target {
                Some(t) if score < t => format!("; target {t:.1} NOT reached"),
                Some(t) => format!("; target {t:.1} reached"),
                None => String::new(),
            };
            log.push(format!(
                "section {label}: {} seeds, best seed {seed} score {score:.2} (worst {worst:.2}; top {}{reached})",
                scored.len(),
                top.join(", ")
            ));
        }
    }
    Ok((best_file, log))
}
