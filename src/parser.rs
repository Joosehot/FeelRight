//! Chord-symbol and key parsing.

use crate::model::Meter;
use crate::theory::{Chord, Key, Mode, PitchClass, Quality};
use anyhow::{anyhow, bail, Context, Result};

fn parse_root(s: &str) -> Result<(PitchClass, &str)> {
    let letter = s.chars().next().ok_or_else(|| anyhow!("empty chord symbol"))?;
    let base = match letter.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => bail!("unknown note letter '{letter}' in '{s}'"),
    };
    let rest = &s[letter.len_utf8()..];
    let (acc, rest) = if let Some(r) = rest.strip_prefix('#') {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (-1, r)
    } else {
        (0, rest)
    };
    Ok((PitchClass::new(base + acc), rest))
}

pub fn parse_chord(sym: &str) -> Result<Chord> {
    let (root, suffix) = parse_root(sym.trim())?;
    let quality = match suffix {
        "" | "maj" | "M" => Quality::Maj,
        "m" | "min" | "-" => Quality::Min,
        "dim" | "o" => Quality::Dim,
        "aug" | "+" => Quality::Aug,
        "7" | "dom7" => Quality::Dom7,
        "maj7" | "M7" => Quality::Maj7,
        "m7" | "min7" | "-7" => Quality::Min7,
        "m7b5" => Quality::HalfDim7,
        "sus2" => Quality::Sus2,
        "sus4" | "sus" => Quality::Sus4,
        other => bail!("unknown chord quality '{other}' in '{sym}'"),
    };
    Ok(Chord::new(root, quality))
}

/// A chord with its position in the piece.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordSpan {
    pub chord: Chord,
    /// Start in grid steps (16ths).
    pub start: u32,
    /// Length in grid steps.
    pub len: u32,
}

/// Parse a progression such as `"Am F C G | Am F E E"`.
///
/// If the number of chord tokens equals `bars`, every token is one bar and
/// `|` is only a phrase mark. Otherwise `|` separates bars and whitespace
/// separates chords within a bar (one or two per bar, splitting it evenly);
/// without any `|`, each token is one bar. The progression is looped or
/// truncated to `bars` bars.
pub fn parse_progression(text: &str, meter: Meter, bars: u32) -> Result<Vec<ChordSpan>> {
    let steps_per_bar = meter.steps_per_bar();
    let tokens: Vec<&str> = text.split(|c: char| c.is_whitespace() || c == '|')
        .filter(|s| !s.is_empty())
        .collect();
    let bar_texts: Vec<&str> = if tokens.len() as u32 == bars || !text.contains('|') {
        tokens
    } else {
        text.split('|').map(str::trim).filter(|s| !s.is_empty()).collect()
    };
    if bar_texts.is_empty() {
        bail!("no chords given");
    }
    let mut pattern: Vec<Vec<Chord>> = Vec::new();
    for bt in &bar_texts {
        let chords: Vec<Chord> = bt
            .split_whitespace()
            .map(|c| parse_chord(c).with_context(|| format!("in bar '{bt}'")))
            .collect::<Result<_>>()?;
        if chords.is_empty() {
            bail!("empty bar in progression");
        }
        if chords.len() > 2 || steps_per_bar % chords.len() as u32 != 0 {
            bail!(
                "bar '{bt}' has {} chords; use one or two per bar",
                chords.len()
            );
        }
        pattern.push(chords);
    }
    let mut out = Vec::new();
    for bar in 0..bars {
        let chords = &pattern[(bar as usize) % pattern.len()];
        let len = steps_per_bar / chords.len() as u32;
        for (i, c) in chords.iter().enumerate() {
            out.push(ChordSpan {
                chord: c.clone(),
                start: bar * steps_per_bar + i as u32 * len,
                len,
            });
        }
    }
    Ok(out)
}

/// Parse `"A:minor"`, `"C:major"`, `"Am"`, `"C"`, `"F#:min"`.
pub fn parse_key(text: &str) -> Result<Key> {
    let text = text.trim();
    let (tonic_s, mode_s) = match text.split_once(':') {
        Some((t, m)) => (t, m.to_ascii_lowercase()),
        None => {
            if let Some(t) = text.strip_suffix('m') {
                (t, "minor".to_string())
            } else {
                (text, "major".to_string())
            }
        }
    };
    let (tonic, rest) = parse_root(tonic_s)?;
    if !rest.is_empty() {
        bail!("unexpected '{rest}' in key '{text}'");
    }
    let mode = match mode_s.as_str() {
        "major" | "maj" | "ionian" => Mode::Major,
        "minor" | "min" | "aeolian" => Mode::Minor,
        other => bail!("unknown mode '{other}'"),
    };
    Ok(Key::new(tonic, mode))
}

/// Parse `"4/4"`, `"3/4"`, `"6/8"`.
pub fn parse_meter(text: &str) -> Result<Meter> {
    let (n, d) = text
        .trim()
        .split_once('/')
        .ok_or_else(|| anyhow!("meter must look like 4/4"))?;
    let num: u8 = n.parse().context("meter numerator")?;
    let den: u8 = d.parse().context("meter denominator")?;
    if num == 0 || !matches!(den, 2 | 4 | 8 | 16) {
        bail!("unsupported meter {text}");
    }
    Ok(Meter { num, den })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords() {
        assert_eq!(
            parse_chord("Am").unwrap(),
            Chord::new(PitchClass(9), Quality::Min)
        );
        assert_eq!(
            parse_chord("F#m7b5").unwrap(),
            Chord::new(PitchClass(6), Quality::HalfDim7)
        );
        assert_eq!(
            parse_chord("Bbmaj7").unwrap(),
            Chord::new(PitchClass(10), Quality::Maj7)
        );
        assert_eq!(parse_chord("G7").unwrap().quality, Quality::Dom7);
        assert!(parse_chord("H").is_err());
        assert!(parse_chord("Cxyz").is_err());
    }

    #[test]
    fn keys() {
        assert_eq!(
            parse_key("A:minor").unwrap(),
            Key::new(PitchClass(9), Mode::Minor)
        );
        assert_eq!(parse_key("Am").unwrap(), Key::new(PitchClass(9), Mode::Minor));
        assert_eq!(parse_key("C").unwrap(), Key::new(PitchClass(0), Mode::Major));
        assert_eq!(parse_key("Eb:major").unwrap().tonic, PitchClass(3));
    }

    #[test]
    fn progression_bars_and_halves() {
        let m = Meter { num: 4, den: 4 };
        // 8 tokens for 8 bars: one chord per bar, `|` is a phrase mark.
        let p = parse_progression("Am F C G | Am F E E", m, 8).unwrap();
        assert_eq!(p.len(), 8);
        assert_eq!(p[0].len, 16);
        assert_eq!(p[6].chord, parse_chord("E").unwrap());
        assert_eq!(p[4].start, 64);
        assert!(parse_progression("C D E | F", m, 2).is_err());

        let p = parse_progression("C G Am F", m, 8).unwrap();
        assert_eq!(p.len(), 8);
        assert_eq!(p[1].len, 16);
        assert_eq!(p[4].chord, p[0].chord); // loops

        let p = parse_progression("C | G Am | F", m, 3).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(p[1].len, 8);
        assert_eq!(p[2].start, 24);
    }
}

/// Parse a fixed melody: whitespace-separated `NAME:DUR` tokens, e.g.
/// `C4:4 D4:2 r:2 Bb4:8`. Durations are grid steps (16ths). `r` is a
/// rest. Bar lines `|` are ignored. `transpose` shifts by semitones.
pub fn parse_melody(text: &str, transpose: i32) -> Result<crate::model::Melody> {
    use crate::model::{Event, Melody, Note};
    let mut m = Melody::default();
    let mut t = 0;
    for tok in text.split_whitespace() {
        if tok == "|" {
            continue;
        }
        let (name, dur) = tok
            .split_once(':')
            .ok_or_else(|| anyhow!("melody token '{tok}' must look like C4:4 or r:2"))?;
        let dur: u32 = dur.parse().with_context(|| format!("duration in '{tok}'"))?;
        if dur == 0 {
            bail!("zero duration in '{tok}'");
        }
        if name.eq_ignore_ascii_case("r") {
            m.push(Event::Rest { start: t, dur });
        } else {
            let (pc, rest) = parse_root(name)?;
            let octave: i32 = rest.parse().with_context(|| format!("octave in '{tok}'"))?;
            let midi = (octave + 1) * 12 + pc.0 as i32 + transpose;
            if !(0..=127).contains(&midi) {
                bail!("note '{tok}' is out of MIDI range");
            }
            m.push(Event::Note(Note { pitch: midi as u8, start: t, dur }));
        }
        t += dur;
    }
    Ok(m)
}

#[cfg(test)]
mod melody_tests {
    use super::*;

    #[test]
    fn fixed_melody() {
        let m = parse_melody("C4:4 r:2 Bb4:2 | G#3:8", 0).unwrap();
        let p: Vec<u8> = m.notes().map(|n| n.pitch).collect();
        assert_eq!(p, vec![60, 70, 56]);
        assert_eq!(m.total_steps(), 16);
        assert_eq!(parse_melody("C4:4", 1).unwrap().notes().next().unwrap().pitch, 61);
        assert!(parse_melody("C4", 0).is_err());
    }
}
