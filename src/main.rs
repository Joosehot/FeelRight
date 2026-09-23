#![allow(dead_code)] // helpers used by later milestones

mod config;
mod form;
mod generate;
mod midi;
mod model;
mod motif;
mod parser;
mod rules;
mod search;
mod tension;
mod theory;

use anyhow::{bail, Context as _, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "melody", version, about = "Deterministic melody engine")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate a melody over a chord progression and write a MIDI file.
    Generate(GenerateArgs),
    /// Generate a multi-section piece described by a TOML file.
    Suite(SuiteArgs),
}

#[derive(clap::Args)]
struct SuiteArgs {
    /// Suite description (see examples/orkesteri.toml)
    #[arg(long)]
    file: PathBuf,
    /// Output MIDI path
    #[arg(long, default_value = "suite.mid")]
    out: PathBuf,
    /// Path to rules.toml
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Print per-section explanation
    #[arg(long)]
    explain: bool,
}

#[derive(serde::Deserialize)]
struct SuiteFile {
    #[serde(default)]
    name: String,
    section: Vec<SuiteSection>,
}

#[derive(serde::Deserialize)]
struct SuiteSection {
    #[serde(default)]
    name: String,
    chords: String,
    key: String,
    #[serde(default = "default_meter")]
    meter: String,
    tempo: u32,
    bars: u32,
    #[serde(default)]
    tension: Option<Vec<f32>>,
    #[serde(default = "default_seed")]
    seed: u64,
    #[serde(default = "default_instrument")]
    instrument: String,
    #[serde(default)]
    octave: i32,
    #[serde(default = "default_style")]
    style: String,
    #[serde(default = "default_form")]
    form: String,
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

#[derive(clap::Args)]
struct GenerateArgs {
    /// Chord progression, e.g. "Am F C G | Am F E E"
    #[arg(long)]
    chords: String,
    /// Key, e.g. "A:minor" or "C"
    #[arg(long)]
    key: String,
    /// Meter, e.g. 4/4
    #[arg(long, default_value = "4/4")]
    meter: String,
    /// Tempo in BPM
    #[arg(long, default_value_t = 100)]
    tempo: u32,
    /// Number of bars
    #[arg(long, default_value_t = 8)]
    bars: u32,
    /// Per-bar tension targets 0.0-1.0, comma separated (not used until M5)
    #[arg(long, value_delimiter = ',')]
    tension: Option<Vec<f32>>,
    /// Random seed
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Number of variants (seeds seed..seed+N), written as out-1.mid, out-2.mid...
    #[arg(long, default_value_t = 1)]
    variants: u32,
    /// Output MIDI path
    #[arg(long, default_value = "out.mid")]
    out: PathBuf,
    /// Print per-bar explanation (not used until M6)
    #[arg(long)]
    explain: bool,
    /// Style preset: rhythm library and accompaniment texture
    #[arg(long, value_enum, default_value_t = model::Style::Classical)]
    style: model::Style,
    /// Path to rules.toml (default: ./rules.toml if present, else built-in)
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Engine: beam search (default) or the random chord-tone baseline
    #[arg(long, value_enum, default_value_t = Engine::Beam)]
    engine: Engine,
    /// Melody instrument (General MIDI name or program number 0-127)
    #[arg(long, default_value = "piano")]
    instrument: String,
    /// Shift the melody by whole octaves in the MIDI output
    #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
    octave: i32,
    /// Form template (auto: sentence for 8 bars, AABA for 16+)
    #[arg(long, value_enum, default_value_t = form::Template::Auto)]
    form: form::Template,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum Engine {
    Beam,
    Random,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Generate(args) => generate(args),
        Cmd::Suite(args) => suite(args),
    }
}

fn variant_path(base: &PathBuf, i: u32, variants: u32) -> PathBuf {
    if variants <= 1 {
        return base.clone();
    }
    let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let ext = base.extension().and_then(|s| s.to_str()).unwrap_or("mid");
    base.with_file_name(format!("{stem}-{i}.{ext}"))
}

fn generate(args: GenerateArgs) -> Result<()> {
    let key = parser::parse_key(&args.key)?;
    let meter = parser::parse_meter(&args.meter)?;
    if args.bars == 0 {
        bail!("--bars must be at least 1");
    }
    let spans = parser::parse_progression(&args.chords, meter, args.bars)?;
    if let Some(t) = &args.tension {
        if t.len() as u32 != args.bars {
            bail!("--tension needs exactly {} values, got {}", args.bars, t.len());
        }
        if t.iter().any(|v| !(0.0..=1.0).contains(v)) {
            bail!("--tension values must be within 0.0..=1.0");
        }
    }

    let cfg = config::Config::load(args.rules.as_deref())?;
    let rule_set = rules::all_rules();
    cfg.validate(&rule_set)?;
    let tension: Vec<f32> = args
        .tension
        .clone()
        .unwrap_or_else(|| tension::default_curve(args.bars));
    let form = form::Form::plan(args.form, args.bars);
    let program = gm_program(&args.instrument)?;

    for i in 0..args.variants {
        let seed = args.seed + i as u64;
        let melody = match args.engine {
            Engine::Random => {
                generate::random_chord_tone_melody(&spans, meter, args.bars, seed, args.style)
            }
            Engine::Beam => {
                let input = search::SearchInput {
                    chords: &spans,
                    key,
                    meter,
                    bars: args.bars,
                    tension: &tension,
                    style: args.style,
                    seed,
                    form: &form,
                };
                search::beam_search(&input, &cfg, &rule_set).0
            }
        };
        let path = variant_path(&args.out, i + 1, args.variants);
        midi::write_midi(&path, &melody, &spans, meter, args.tempo, args.style, &tension, program, args.octave, &form.phrase_ends())?;
        let ctx = rules::Context {
            melody: &melody,
            chords: &spans,
            key,
            meter,
            bars: args.bars,
            tension: &tension,
            complete: true,
            form: &form,
        };
        let eval = rules::evaluate(&ctx, &cfg, &rule_set);
        let note_count = melody.notes().count();
        println!(
            "wrote {} ({key}, {} bars, {} notes, seed {seed}, {:?}, {:?}, {:?}) score {:.2}{}",
            path.display(),
            args.bars,
            note_count,
            args.style,
            args.engine,
            form.template,
            eval.total,
            if eval.hard_violation { " [HARD VIOLATION]" } else { "" }
        );
        if args.explain {
            print_explanation(&eval, &tension, &cfg);
        }
    }
    Ok(())
}

fn suite(args: SuiteArgs) -> Result<()> {
    use clap::ValueEnum;
    let text = std::fs::read_to_string(&args.file)
        .with_context(|| format!("reading {}", args.file.display()))?;
    let file: SuiteFile = toml::from_str(&text).context("parsing suite file")?;
    let cfg = config::Config::load(args.rules.as_deref())?;
    let rule_set = rules::all_rules();
    cfg.validate(&rule_set)?;

    struct Rendered {
        melody: model::Melody,
        chords: Vec<parser::ChordSpan>,
        meter: model::Meter,
        tempo: u32,
        style: model::Style,
        energy: Vec<f32>,
        program: u8,
        octave: i32,
        phrase_ends: Vec<u32>,
        offset: u32,
    }
    let mut rendered: Vec<Rendered> = Vec::new();
    let mut offset = 0;
    for (i, sec) in file.section.iter().enumerate() {
        let key = parser::parse_key(&sec.key)?;
        let meter = parser::parse_meter(&sec.meter)?;
        let spans = parser::parse_progression(&sec.chords, meter, sec.bars)?;
        let tension = sec.tension.clone().unwrap_or_else(|| tension::default_curve(sec.bars));
        if tension.len() as u32 != sec.bars {
            bail!("section {}: tension needs {} values", i + 1, sec.bars);
        }
        let style = model::Style::from_str(&sec.style, true).map_err(|e| anyhow::anyhow!(e))?;
        let template = form::Template::from_str(&sec.form, true).map_err(|e| anyhow::anyhow!(e))?;
        let form = form::Form::plan(template, sec.bars);
        let input = search::SearchInput {
            chords: &spans,
            key,
            meter,
            bars: sec.bars,
            tension: &tension,
            style,
            seed: sec.seed,
            form: &form,
        };
        let (melody, eval) = search::beam_search(&input, &cfg, &rule_set);
        let label = if sec.name.is_empty() { format!("{}", i + 1) } else { sec.name.clone() };
        println!(
            "section {label}: {} bars, {key}, {} BPM, {}, score {:.2}",
            sec.bars, sec.tempo, sec.instrument, eval.total
        );
        if args.explain {
            print_explanation(&eval, &tension, &cfg);
        }
        rendered.push(Rendered {
            melody,
            chords: spans,
            meter,
            tempo: sec.tempo,
            style,
            energy: tension,
            program: gm_program(&sec.instrument)?,
            octave: sec.octave,
            phrase_ends: form.phrase_ends(),
            offset,
        });
        offset += sec.bars * meter.steps_per_bar();
    }
    let sections: Vec<midi::Section> = rendered
        .iter()
        .map(|r| midi::Section {
            melody: &r.melody,
            chords: &r.chords,
            meter: r.meter,
            tempo_bpm: r.tempo,
            style: r.style,
            energy: &r.energy,
            program: r.program,
            octave: r.octave,
            phrase_ends: &r.phrase_ends,
            offset: r.offset,
        })
        .collect();
    midi::write_suite(&args.out, &sections)?;
    println!("wrote {} ({}, {} sections, {} bars)", args.out.display(), file.name, sections.len(), offset / 16);
    Ok(())
}

/// M6 explanation. Per bar: target vs observed tension, the top positive
/// and negative contributions, and broken rules with why they paid off.
/// Then a per-rule summary.
fn print_explanation(eval: &rules::Evaluation, target: &[f32], cfg: &config::Config) {
    let bars = eval.observed.len();
    for bar in 0..bars {
        let t = target.get(bar).copied().unwrap_or(0.5);
        let o = eval.observed[bar];
        println!("Bar {:<2} target {t:.2}  observed {o:.2}", bar + 1);
        // This bar's details, weighted by the rule weight.
        let mut items: Vec<(f32, String, &str, bool)> = Vec::new();
        for r in &eval.rules {
            for d in &r.result.details {
                if d.bar as usize != bar {
                    continue;
                }
                items.push((d.score * r.weight, d.text.clone(), r.name, r.breakable));
            }
        }
        let mut pos: Vec<_> = items.iter().filter(|i| i.0 > 0.0).collect();
        let mut neg: Vec<_> = items.iter().filter(|i| i.0 < 0.0).collect();
        pos.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        neg.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for (w, text, name, _) in pos.iter().take(3) {
            let tag = if is_tension_rule(name) { "  [tension]" } else { "" };
            println!("  + {:<44} {:+.1}{tag}", text, w);
        }
        for (w, text, _, breakable) in neg.iter().take(3) {
            let allowed = *breakable && cfg.search.breakable_discount > 0.0 && t >= 0.6;
            let note = if allowed { "  (allowed, tension high)" } else { "" };
            println!("  - {:<44} {:+.1}{note}", text, w);
        }
    }
    println!("Tension term {:+.2} (lambda {})", eval.tension_term, cfg.search.tension_lambda);
    println!("Rules:");
    for r in &eval.rules {
        if r.adjusted == 0.0 && !r.result.broken {
            continue;
        }
        let flag = if r.result.broken { " broken" } else { "" };
        let forgiven = if r.forgiven > 0.0 { format!(" forgiven {:+.2}", r.forgiven * r.weight) } else { String::new() };
        println!("  {:<28} {:+.2} (score {:+.2} x weight {:.1}){flag}{forgiven}", r.name, r.weighted(), r.result.score, r.weight);
    }
    println!("Total {:.2}", eval.total);
}

/// General MIDI program for a few common instrument names, or a number.
fn gm_program(name: &str) -> Result<u8> {
    let n = name.trim().to_ascii_lowercase();
    Ok(match n.as_str() {
        "piano" => 0,
        "harpsichord" => 6,
        "guitar" => 24,
        "violin" => 40,
        "viola" => 41,
        "cello" => 42,
        "strings" => 48,
        "trumpet" => 56,
        "trombone" => 57,
        "tuba" => 58,
        "muted_trumpet" => 59,
        "horn" => 60,
        "brass" => 61,
        "timpani" => 47,
        "oboe" => 68,
        "clarinet" => 71,
        "flute" => 73,
        _ => n.parse::<u8>().map_err(|_| anyhow::anyhow!("unknown instrument '{name}'"))?,
    })
}

fn is_tension_rule(name: &str) -> bool {
    matches!(name, "nct_types" | "delayed_resolution" | "syncopation" | "arrival")
}
