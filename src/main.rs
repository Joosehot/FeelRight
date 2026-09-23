#![allow(dead_code)] // helpers used by later milestones

mod config;
mod form;
mod generate;
mod midi;
mod model;
mod motif;
mod parser;
mod prompt;
mod rules;
mod search;
mod suite;
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
    /// Translate a short text description into a suite, search seeds with
    /// the evaluator, and render the best result.
    Prompt(PromptArgs),
}

#[derive(clap::Args)]
struct PromptArgs {
    /// Description, e.g. "mahtipontinen orkesterikappale d-molli ABC viulu"
    text: String,
    /// Output MIDI path
    #[arg(long, default_value = "prompt.mid")]
    out: PathBuf,
    /// Seeds to try per section (evaluator picks the best)
    #[arg(long, default_value_t = 6)]
    seeds: u64,
    /// Path to rules.toml
    #[arg(long)]
    rules: Option<PathBuf>,
    /// Also write the chosen suite as TOML next to the MIDI
    #[arg(long)]
    save_suite: bool,
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
    /// Try this many seeds per section and keep the best before rendering
    #[arg(long, default_value_t = 1)]
    explore: u64,
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
        Cmd::Suite(args) => suite_cmd(args),
        Cmd::Prompt(args) => prompt_cmd(args),
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
    let program = suite::gm_program(&args.instrument)?;

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
                    ends_open: false,
                    theme: None,
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
            ends_open: false,
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

fn report(file: &suite::SuiteFile, rendered: &[suite::Rendered], out: &std::path::Path, explain: bool, cfg: &config::Config) {
    for (sec, r) in file.section.iter().zip(rendered) {
        println!(
            "section {}: {} bars, {}, {} BPM, {}, seed {}, score {:.2}{}",
            sec.name, sec.bars, sec.key, sec.tempo, sec.instrument, sec.seed, r.eval.total,
            if sec.ends_open { " (open)" } else { "" }
        );
        if explain {
            print_explanation(&r.eval, &r.energy, cfg);
        }
    }
    println!(
        "wrote {} ({}, {} sections, {} bars)",
        out.display(), file.name, file.section.len(), suite::total_bars(file)
    );
}

fn suite_cmd(args: SuiteArgs) -> Result<()> {
    let mut file = suite::load(&args.file)?;
    let cfg = config::Config::load(args.rules.as_deref())?;
    let rule_set = rules::all_rules();
    cfg.validate(&rule_set)?;
    if args.explore > 1 {
        let (best, log) = suite::explore(&file, args.explore, &cfg, &rule_set)?;
        for l in log {
            println!("{l}");
        }
        file = best;
    }
    let rendered = suite::render(&file, &args.out, &cfg, &rule_set)?;
    report(&file, &rendered, &args.out, args.explain, &cfg);
    Ok(())
}

fn prompt_cmd(args: PromptArgs) -> Result<()> {
    let plan = prompt::plan(&args.text);
    println!(
        "plan: {} {:?}, {} BPM, {}, style {}, sections {}, instruments {}",
        plan.tonic.name(),
        plan.mode,
        plan.tempo,
        plan.meter,
        plan.style,
        plan.sections.iter().collect::<String>(),
        plan.instruments.join("/")
    );
    for n in &plan.notes {
        println!("  {n}");
    }
    let file = prompt::to_suite(&plan);
    let cfg = config::Config::load(args.rules.as_deref())?;
    let rule_set = rules::all_rules();
    cfg.validate(&rule_set)?;
    let (best, log) = suite::explore(&file, args.seeds.max(1), &cfg, &rule_set)?;
    for l in log {
        println!("{l}");
    }
    let rendered = suite::render(&best, &args.out, &cfg, &rule_set)?;
    report(&best, &rendered, &args.out, false, &cfg);
    if args.save_suite {
        let toml_path = args.out.with_extension("toml");
        suite::save(&toml_path, &best)?;
        println!("saved suite to {}", toml_path.display());
    }
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

fn is_tension_rule(name: &str) -> bool {
    matches!(name, "nct_types" | "delayed_resolution" | "syncopation" | "arrival")
}
