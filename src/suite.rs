//! Multi-section pieces: the suite file format, rendering, and a seed
//! search per section driven by the evaluator.

use crate::config::Config;
use crate::form::{Form, Template};
use crate::model::{Melody, Meter, Style};
use crate::parser::{self, ChordSpan};
use crate::rules::{self, Evaluation, Rule};
use crate::search::{self, SearchInput};
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
        _ => n.parse::<u8>().map_err(|_| anyhow::anyhow!("unknown instrument '{name}'"))?,
    })
}

pub struct Rendered {
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
}

/// Generate (or parse) one section's melody and score it.
pub fn render_section(sec: &SuiteSection, cfg: &Config, rules: &[Box<dyn Rule>]) -> Result<Rendered> {
    let key = parser::parse_key(&sec.key)?;
    let meter = parser::parse_meter(&sec.meter)?;
    let spans = parser::parse_progression(&sec.chords, meter, sec.bars)?;
    let tension = sec.tension.clone().unwrap_or_else(|| tension::default_curve(sec.bars));
    if tension.len() as u32 != sec.bars {
        bail!("section '{}': tension needs {} values, got {}", sec.name, sec.bars, tension.len());
    }
    let style = Style::from_str(&sec.style, true).map_err(|e| anyhow::anyhow!(e))?;
    let template = Template::from_str(&sec.form, true).map_err(|e| anyhow::anyhow!(e))?;
    let form = Form::plan(template, sec.bars);
    let input = SearchInput {
        chords: &spans,
        key,
        meter,
        bars: sec.bars,
        tension: &tension,
        style,
        seed: sec.seed,
        form: &form,
        ends_open: sec.ends_open,
    };
    let (melody, eval) = match &sec.melody {
        Some(text) => {
            let melody = parser::parse_melody(text, sec.transpose)?;
            let expected = sec.bars * meter.steps_per_bar();
            if melody.total_steps() != expected {
                bail!(
                    "section '{}': fixed melody is {} steps, but {} bars need {}",
                    sec.name, melody.total_steps(), sec.bars, expected
                );
            }
            let ctx = rules::Context {
                melody: &melody,
                chords: &spans,
                key,
                meter,
                bars: sec.bars,
                tension: &tension,
                complete: true,
                form: &form,
                ends_open: sec.ends_open,
            };
            let eval = rules::evaluate(&ctx, cfg, rules);
            (melody, eval)
        }
        None => search::beam_search(&input, cfg, rules),
    };
    Ok(Rendered {
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
    })
}

/// Render every section and write the MIDI. Returns the rendered
/// sections for reporting.
pub fn render(file: &SuiteFile, out: &Path, cfg: &Config, rules: &[Box<dyn Rule>]) -> Result<Vec<Rendered>> {
    let mut rendered = Vec::new();
    for sec in &file.section {
        rendered.push(render_section(sec, cfg, rules)?);
    }
    let mut offset = 0;
    let mut sections = Vec::new();
    for r in &rendered {
        sections.push(midi::Section {
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
        });
        offset += r.melody.total_steps();
    }
    midi::write_suite(out, &sections)?;
    Ok(rendered)
}

pub fn total_bars(file: &SuiteFile) -> u32 {
    file.section.iter().map(|s| s.bars).sum()
}

/// For every generated section, try `seeds` seeds and keep the best by
/// evaluator score. Returns the improved suite and a log line per section.
pub fn explore(file: &SuiteFile, seeds: u64, cfg: &Config, rules: &[Box<dyn Rule>]) -> Result<(SuiteFile, Vec<String>)> {
    let mut best_file = file.clone();
    let mut log = Vec::new();
    for (i, sec) in file.section.iter().enumerate() {
        if sec.melody.is_some() {
            continue;
        }
        let mut best: Option<(u64, f32)> = None;
        let mut tried = Vec::new();
        for s in 0..seeds {
            let seed = sec.seed + s;
            let mut trial = sec.clone();
            trial.seed = seed;
            let r = render_section(&trial, cfg, rules)?;
            tried.push((seed, r.eval.total));
            if best.map(|b| r.eval.total > b.1).unwrap_or(true) {
                best = Some((seed, r.eval.total));
            }
        }
        if let Some((seed, score)) = best {
            best_file.section[i].seed = seed;
            let label = if sec.name.is_empty() { format!("{}", i + 1) } else { sec.name.clone() };
            let worst = tried.iter().map(|t| t.1).fold(f32::INFINITY, f32::min);
            log.push(format!("section {label}: best seed {seed} score {score:.2} (worst {worst:.2})"));
        }
    }
    Ok((best_file, log))
}
