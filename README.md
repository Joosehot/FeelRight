# FeelRight

FeelRight is a small, deterministic melody engine (binary name `melody`). Given a chord progression, a key and
an optional tension curve, it searches for the best melody with a
hand-crafted evaluation function (35 rules, each in its own file under
`src/rules/`, weights in `rules.toml`) and bar-by-bar beam search. No
neural nets, no LLM, no audio.

The core mechanic: rules are simple and the engine knows when to break
them. Breaking a breakable rule adds tension; the engine spends that
tension where the curve asks for it.

## Build and test

```
cargo build --release
cargo test
```

## Usage

```
melody generate \
  --chords "Am Dm E7 Am | F Dm E7 Am" \
  --key A:minor --meter 4/4 --tempo 84 --bars 8 \
  --tension 0.2,0.3,0.4,0.6,0.3,0.5,0.9,0.2 \
  --seed 42 --variants 3 \
  --out out.mid --explain
```

| Flag | Meaning |
| --- | --- |
| `--chords` | One chord per bar when the token count equals `--bars` (then `\|` is a phrase mark). Otherwise `\|` separates bars with one or two chords each. Shorter progressions loop. |
| `--key` | `A:minor`, `C:major`, `Am`, `F#` |
| `--tension` | One value 0..1 per bar. Default: an arch peaking at 70 %. |
| `--style` | `classical` (even/dotted rhythms, Alberti bass) or `pop` (syncopated rhythms, block chords) |
| `--form` | `auto`, `sentence`, `period`, `aaba` |
| `--engine` | `beam` (default) or `random` (M1 baseline) |
| `--rules` | Path to a `rules.toml`; defaults to `./rules.toml`, else the built-in copy |
| `--variants N` | Writes `out-1.mid` .. `out-N.mid` with seeds `seed..seed+N` |
| `--explain` | Per-bar target vs observed tension, top contributions, broken rules |

Output MIDI has a conductor track, one melody track per section (velocity
follows the tension curve and the phrase shape), accompaniment, bass and,
for orchestral styles, a second accompaniment layer.

## Suites: multi-section pieces

```
melody suite --file examples/orkesteri.toml --out orkesteri.mid --explore 6 --explain
```

A suite file lists `[[section]]` tables with `chords`, `key`, `meter`,
`tempo`, `bars`, `tension`, `seed`, `instrument`, `octave`, `style`
(`classical`, `pop`, `orchestral`, `brass`, `concerto`, `waltz`, `rapids`),
`form`, and optionally `melody` (a fixed tune in `C4:4 D4:2 r:2` notation,
with `transpose`), `ends_open = true` when the section leads into the
next one, `bridge = N` to append N bars on the dominant of the next
section's key, `theme_from = "A"` to borrow section A's opening motif
(transformed to this section's key), and `rules_override` for per-section
weight or parameter changes, e.g.
`rules_override = { max_leap = { soft_max = 12 }, density = { weight = 3.0 } }`.
`--explore N` tries N seeds per section and keeps the best by evaluator
score. See `examples/*.toml`.

## Prompt translator

```
melody prompt "mahtipontinen orkesterikappale d-molli ABC viulu" --seeds 6 --out piece.mid --save-suite
```

A deterministic keyword mapper (Finnish and English, no language model)
turns a short description into a suite: mood words set mode, tempo and
energy; style words pick the accompaniment; instrument names, a key such
as `d-molli` or `Bb major`, a `120 bpm` tempo and section letters (`ABC`,
`ABA`, `ABCA`) are honoured. The evaluator then searches seeds per
section, renders the best, and `--save-suite` writes the chosen suite as
TOML for hand editing.

## Working with an AI assistant

The engine itself contains no neural network and no language model. It is
a deterministic instrument: the same suite file and seeds always produce
the same notes, and every note is checked and explained by the rules.

The intended workflow puts the AI *outside* the engine, as the composer
who writes its instructions:

1. **You describe the music** in plain language to an assistant such as
   Claude Code, e.g. "a Dvořák-style brass piece, chorale, dance, tutti".
2. **The assistant writes a suite file** (`examples/*.toml`): chord
   progressions, keys, tempos, forms, instruments, per-bar tension curves,
   `theme_from` links between sections, bridges, and `rules_override`
   tables that bend the rules toward the style (allow wide leaps for an
   aria, forbid syncopation for Bach, reward sequences, and so on).
3. **The engine renders it**: `melody suite --file piece.toml --explore 8
   --out piece.mid`. Seed exploration and refinement pick and polish the
   melody by the evaluator's score.
4. **The assistant reads `--explain`** (target vs observed tension per
   bar, broken rules, weak spots) and revises the suite. You listen and
   steer: "the middle section should breathe more", "make the ending
   grander". Repeat.

Every piece in `examples/` was made exactly this way in one session with
Claude Code. The assistant never wrote a note; it wrote instructions, the
engine wrote the notes.

`melody prompt "..."` is a small built-in stand-in for step 2 (a keyword
table, Finnish and English) for use without an assistant. `melody rate`
and `melody tune` close the loop on taste: your A/B choices and a corpus
of known melodies re-weight `rules.toml`.

A minimal script that automates step 2 with the Claude API:

```python
import anthropic, subprocess
client = anthropic.Anthropic()
context = open("README.md").read() + open("examples/orkesteri2.toml").read()
msg = client.messages.create(
    model="claude-sonnet-5", max_tokens=2000,
    system="You write FeelRight suite files. Reply with TOML only.
" + context,
    messages=[{"role": "user", "content": "a sad cello piece, ABA, E minor"}],
)
open("piece.toml", "w").write(msg.content[0].text)
subprocess.run(["melody", "suite", "--file", "piece.toml", "--out", "piece.mid", "--explore", "8"])
```

## Layout

- `src/theory.rs`, `src/parser.rs`, `src/model.rs` — pitch classes, chords, keys, grid, notes
- `src/generate.rs` — rhythm libraries and the random baseline
- `src/motif.rs` — motif extraction and transformations
- `src/form.rs` — form templates and bar roles
- `src/search.rs` — beam search
- `src/rules/` — one file per rule, `Rule` trait in `mod.rs`
- `src/tension.rs` — default curve, observed tension, match term
- `src/suite.rs` — suite file format, rendering, seed exploration
- `src/prompt.rs` — keyword translator from text to suite
- `src/midi.rs` — MIDI writer
- `rules.toml` — all weights and parameters
- `examples/out/` — generated examples with their explanations
