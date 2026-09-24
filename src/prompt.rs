//! Prompt translator: a deterministic keyword mapper from a short text
//! description (Finnish or English) to a suite description. No language
//! model; every decision is a lookup in the tables below.

use crate::suite::{SuiteFile, SuiteSection};
use crate::theory::{Mode, PitchClass};

#[derive(Clone, Debug)]
pub struct Plan {
    pub name: String,
    pub mode: Mode,
    pub tonic: PitchClass,
    pub tempo: u32,
    pub meter: String,
    pub style: String,
    pub instruments: Vec<String>,
    pub sections: Vec<char>,
    pub bars: u32,
    pub energy: f32,
    pub notes: Vec<String>,
}

fn has(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| t.contains(w))
}

fn number_after(t: &str, unit: &str) -> Option<u32> {
    for (i, tok) in t.split_whitespace().enumerate() {
        if tok.starts_with(unit) || t.split_whitespace().nth(i + 1).map(|n| n.starts_with(unit)).unwrap_or(false) {
            if let Ok(n) = tok.trim_end_matches(|c: char| !c.is_ascii_digit()).parse::<u32>() {
                return Some(n);
            }
        }
    }
    None
}

/// Find a key like "d-molli", "D minor", "Es-duuri", "Bb major".
fn find_key(t: &str) -> Option<(PitchClass, Mode)> {
    let names: &[(&str, i32)] = &[
        ("c#", 1), ("cis", 1), ("db", 1), ("des", 1),
        ("d#", 3), ("dis", 3), ("eb", 3), ("es", 3),
        ("f#", 6), ("fis", 6), ("gb", 6), ("ges", 6),
        ("g#", 8), ("gis", 8), ("ab", 8), ("as", 8),
        ("a#", 10), ("ais", 10), ("bb", 10), ("b", 10),
        ("h", 11), ("c", 0), ("d", 2), ("e", 4), ("f", 5), ("g", 7), ("a", 9),
    ];
    let modes: &[(&str, Mode)] = &[
        ("-molli", Mode::Minor), (" molli", Mode::Minor), ("molli", Mode::Minor), (" minor", Mode::Minor),
        ("-duuri", Mode::Major), (" duuri", Mode::Major), ("duuri", Mode::Major), (" major", Mode::Major),
    ];
    for tok in t.split_whitespace() {
        for (suffix, mode) in modes {
            let suffix = suffix.trim();
            if let Some(stem) = tok.strip_suffix(suffix) {
                let stem = stem.trim_end_matches('-');
                for (n, pc) in names {
                    if stem == *n {
                        return Some((PitchClass::new(*pc), *mode));
                    }
                }
            }
        }
    }
    // "D minor" as two tokens.
    let toks: Vec<&str> = t.split_whitespace().collect();
    for w in toks.windows(2) {
        let mode = match w[1] {
            "minor" | "molli" | "moll" => Mode::Minor,
            "major" | "duuri" | "dur" => Mode::Major,
            _ => continue,
        };
        for (n, pc) in names {
            if w[0] == *n {
                return Some((PitchClass::new(*pc), mode));
            }
        }
    }
    None
}

pub fn plan(text: &str) -> Plan {
    let t = text.to_lowercase();
    let mut notes = Vec::new();

    // Mood -> mode, energy, tempo baseline.
    let mut mode = Mode::Major;
    let mut energy: f32 = 0.6;
    let mut tempo = 100;
    if has(&t, &["surullinen", "sad", "melankol", "elegia", "elegy", "haikea", "kaiho", "molli", "minor", "nokturno", "nocturne", "synkkä", "dark"]) {
        mode = Mode::Minor;
        energy = 0.45;
        tempo = 76;
        notes.push("mood: minor, calmer".into());
    }
    if has(&t, &["iloinen", "happy", "kirkas", "bright", "juhla", "festive", "syntymäpäivä", "birthday"]) {
        mode = Mode::Major;
        energy = 0.65;
        tempo = 112;
        notes.push("mood: major, bright".into());
    }
    if has(&t, &["mahtipont", "grand", "eeppinen", "epic", "majestic", "maestoso", "voimakas", "heroic", "sankar"]) {
        energy = 0.85;
        tempo = tempo.max(104);
        notes.push("mood: grand".into());
    }
    if has(&t, &["rauhallinen", "calm", "hiljainen", "quiet", "lempeä", "gentle", "kehtolaulu", "lullaby"]) {
        energy = 0.3;
        tempo = 68;
        notes.push("mood: calm".into());
    }
    if has(&t, &["nopea", "fast", "vauhdikas", "presto", "allegro", "energinen", "energetic"]) {
        tempo = tempo.max(140);
        notes.push("tempo: fast".into());
    }
    if has(&t, &["hidas", "slow", "adagio", "largo", "lento"]) {
        tempo = tempo.min(66);
        notes.push("tempo: slow".into());
    }

    // Style and meter.
    let mut style = "classical".to_string();
    let mut meter = "4/4".to_string();
    let mut instruments: Vec<String> = Vec::new();
    if has(&t, &["orkesteri", "orchestr", "sinfon", "symphon", "elokuva", "film", "soundtrack"]) {
        style = "orchestral".into();
        instruments = vec!["strings".into(), "oboe".into(), "horn".into()];
    }
    if has(&t, &["vaski", "brass", "fanfaari", "fanfare", "marssi", "march", "star wars", "avaruus"]) {
        style = "brass".into();
        instruments = vec!["trumpet".into(), "strings".into(), "horn".into()];
        energy = energy.max(0.7);
        tempo = tempo.max(108);
    }
    if has(&t, &["konsertto", "concerto", "tchaikovsky", "tšaikovski", "rachmaninov"]) {
        style = "concerto".into();
        instruments = vec!["horn".into(), "piano".into(), "strings".into()];
        energy = energy.max(0.7);
    }
    if has(&t, &["valssi", "waltz", "karuselli", "howl", "3/4"]) {
        style = "waltz".into();
        meter = "3/4".into();
        instruments = vec!["piano".into(), "strings".into(), "piano".into()];
        tempo = tempo.max(150);
    }
    if has(&t, &["koski", "rapids", "kaski", "preludi", "prelude", "arpeggio"]) {
        style = "rapids".into();
        instruments = vec!["piano".into(), "piano".into(), "piano".into()];
        tempo = tempo.max(120);
    }
    if has(&t, &["bach", "barokki", "baroque", "preludi bwv", "fuuga", "fugue", "cembalo", "harpsichord"]) {
        style = "baroque".into();
        instruments = vec!["harpsichord".into(), "harpsichord".into(), "harpsichord".into()];
        tempo = tempo.max(96);
        notes.push("style: baroque".into());
    }
    if has(&t, &["hassu", "hauska", "funny", "sirkus", "circus", "komiikka", "comic", "klovni", "clown"]) {
        style = "circus".into();
        instruments = vec!["bassoon".into(), "piccolo".into(), "xylophone".into()];
        mode = Mode::Major;
        tempo = tempo.max(132);
        energy = energy.max(0.6);
        notes.push("mood: comic".into());
    }
    if has(&t, &["tango", "habanera", "milonga"]) {
        style = "tango".into();
        instruments = vec!["bandoneon".into(), "violin".into(), "bandoneon".into()];
        tempo = tempo.max(120);
        notes.push("style: tango".into());
    }
    if has(&t, &["jazz", "swing", "bebop", "blues"]) {
        style = "jazz".into();
        instruments = vec!["sax".into(), "epiano".into(), "trumpet".into()];
        tempo = tempo.max(132);
    }
    if has(&t, &["pop", "cantopop", "iskelmä", "schlager"]) {
        style = "pop".into();
        instruments = vec!["piano".into(), "flute".into(), "flute".into()];
    }
    if has(&t, &["schindler", "elegia", "elegy"]) {
        style = "orchestral".into();
        instruments = vec!["violin".into(), "violin".into(), "violin".into()];
        mode = Mode::Minor;
        tempo = 60;
        energy = 0.4;
    }

    // Explicit instruments override the melody instrument of every section.
    let named: &[(&[&str], &str)] = &[
        (&["viulu", "violin"], "violin"),
        (&["sello", "cello"], "cello"),
        (&["huilu", "flute"], "flute"),
        (&["oboe", "oboa"], "oboe"),
        (&["klarinetti", "clarinet"], "clarinet"),
        (&["trumpetti", "trumpet"], "trumpet"),
        (&["käyrätorvi", "horn"], "horn"),
        (&["pasuuna", "trombone"], "trombone"),
        (&["kitara", "guitar"], "guitar"),
        (&["piano"], "piano"),
        (&["jouset", "strings"], "strings"),
        (&["laulu", "voice", "tenori", "tenor", "vocal"], "voice"),
        (&["saksofoni", "sax"], "sax"),
    ];
    let mut explicit: Vec<String> = Vec::new();
    for (words, inst) in named {
        if has(&t, words) {
            explicit.push(inst.to_string());
        }
    }
    if !explicit.is_empty() {
        // Lead instrument for A and C, second instrument (if any) for B.
        let lead = explicit[0].clone();
        let second = explicit.get(1).cloned().unwrap_or_else(|| lead.clone());
        instruments = vec![lead.clone(), second, lead];
    }
    if instruments.is_empty() {
        instruments = vec!["piano".into(), "piano".into(), "piano".into()];
    }

    // Key.
    let (tonic, key_mode) = find_key(&t).unwrap_or((
        match mode {
            Mode::Minor => PitchClass::new(2), // D minor
            Mode::Major => PitchClass::new(0), // C major
        },
        mode,
    ));
    mode = key_mode;

    // Tempo given explicitly.
    if let Some(n) = number_after(&t, "bpm") {
        tempo = n;
    }

    // Sections.
    let mut sections: Vec<char> = vec!['A', 'B', 'C'];
    if has(&t, &["aba", "a-b-a"]) {
        sections = vec!['A', 'B', 'A'];
    } else if has(&t, &["abca", "abc a"]) {
        sections = vec!['A', 'B', 'C', 'A'];
    } else if has(&t, &["abc"]) {
        sections = vec!['A', 'B', 'C'];
    } else if has(&t, &["yksi osa", "one section", "lyhyt", "short"]) {
        sections = vec!['A'];
    }
    let bars = if has(&t, &["pitkä", "long"]) { 32 } else { 16 };

    Plan {
        name: text.trim().to_string(),
        mode,
        tonic,
        tempo,
        meter,
        style,
        instruments,
        sections,
        bars,
        energy,
        notes,
    }
}

/// Chord progressions per section role and mode, written in C / A minor
/// and transposed to the plan's key.
fn progression(role: char, mode: Mode, bars: u32) -> String {
    let (a, b, c) = match mode {
        Mode::Major => (
            "C G Am F | C G F G | Am Em F C | F C G C",
            "F G Em Am | F G C C | Dm G Em Am | F G G7 G7",
            "C G Am F | C G F G | Am Em F C | F G C C",
        ),
        Mode::Minor => (
            "Am Dm E7 Am | F Dm E7 Am | Am F C G | Dm Am E7 Am",
            "C G Am F | C G F G | Am F C G | F G E7 E7",
            "Am Dm E7 Am | F Dm E7 Am | Am F C G | Dm E7 Am Am",
        ),
    };
    let text = match role {
        'A' => a,
        'B' => b,
        _ => c,
    };
    let mut bars_text: Vec<&str> = text.split('|').map(str::trim).collect();
    let per_phrase = 4;
    let want = (bars / per_phrase) as usize;
    while bars_text.len() < want {
        let extra = bars_text.clone();
        bars_text.extend(extra);
    }
    bars_text.truncate(want.max(1));
    bars_text.join(" | ")
}

fn transpose_progression(text: &str, semis: i32) -> String {
    text.split_whitespace()
        .map(|tok| {
            if tok == "|" {
                return tok.to_string();
            }
            let (root, rest) = split_root(tok);
            let pc = PitchClass::new(root + semis);
            format!("{}{}", pc.name(), rest)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_root(tok: &str) -> (i32, &str) {
    let mut chars = tok.chars();
    let letter = chars.next().unwrap();
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => 0,
    };
    let rest = &tok[1..];
    if let Some(r) = rest.strip_prefix('#') {
        (base + 1, r)
    } else if let Some(r) = rest.strip_prefix('b') {
        (base - 1, r)
    } else {
        (base, rest)
    }
}

fn curve(role: char, bars: u32, energy: f32, last: bool) -> Vec<f32> {
    let base = crate::tension::default_curve(bars);
    let lift = match role {
        'A' => 0.0,
        'B' => -0.1,
        _ => 0.15,
    };
    let scale = 0.6 + 0.6 * energy;
    let mut v: Vec<f32> = base.iter().map(|x| ((x + lift) * scale).clamp(0.05, 1.0)).collect();
    if !last {
        // Lead into the next section: end open and rising.
        let n = v.len();
        v[n - 1] = (v[n - 2] + 0.15).clamp(0.3, 0.9);
    } else {
        let n = v.len();
        v[n - 1] = 0.1;
    }
    v
}

pub fn to_suite(plan: &Plan) -> SuiteFile {
    let home = match plan.mode {
        Mode::Major => 0,
        Mode::Minor => 9,
    };
    let semis = plan.tonic.0 as i32 - home;
    let key_name = |mode: Mode| -> String {
        let t = match mode {
            Mode::Major => plan.tonic,
            // Relative major of the home minor key, for B sections.
            Mode::Minor => plan.tonic,
        };
        format!("{}:{}", t.name(), if mode == Mode::Major { "major" } else { "minor" })
    };
    let n = plan.sections.len();
    let mut sections = Vec::new();
    for (i, &role) in plan.sections.iter().enumerate() {
        let last = i + 1 == n;
        let bars = if role == 'B' { plan.bars.min(16) } else { plan.bars };
        // B sections go to the relative key.
        let (chords, key) = if role == 'B' {
            let rel_mode = match plan.mode {
                Mode::Major => Mode::Major,
                Mode::Minor => Mode::Major,
            };
            let prog = progression('B', plan.mode, bars);
            let key = match plan.mode {
                Mode::Minor => format!("{}:major", plan.tonic.add(3).name()),
                Mode::Major => key_name(rel_mode),
            };
            (transpose_progression(&prog, semis), key)
        } else {
            (transpose_progression(&progression(role, plan.mode, bars), semis), key_name(plan.mode))
        };
        let tempo = match role {
            'B' => (plan.tempo as f32 * 0.9) as u32,
            'C' => (plan.tempo as f32 * 1.06) as u32,
            _ => plan.tempo,
        };
        let form = match role {
            'A' | 'C' if bars >= 16 => "aaba",
            'B' => "sentence",
            _ => "period",
        };
        let instrument = plan.instruments.get(i.min(2)).cloned().unwrap_or_else(|| "piano".into());
        let octave = if matches!(instrument.as_str(), "violin" | "flute" | "strings" | "piano") { 1 } else { 0 };
        sections.push(SuiteSection {
            name: format!("{role}"),
            chords,
            key,
            meter: plan.meter.clone(),
            tempo,
            bars,
            tension: Some(curve(role, bars, plan.energy, last)),
            seed: 1 + i as u64,
            instrument,
            octave,
            style: plan.style.clone(),
            form: form.into(),
            melody: None,
            transpose: 0,
            ends_open: !last,
            theme_from: if role == 'C' && plan.sections.contains(&'A') { Some("A".into()) } else { None },
            bridge: if last { 0 } else { 1 },
            rules_override: None,
            refine: 40,
        });
    }
    SuiteFile { name: plan.name.clone(), section: sections }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_map_to_plan() {
        let p = plan("mahtipontinen orkesterikappale d-molli ABC viulu");
        assert_eq!(p.mode, Mode::Minor);
        assert_eq!(p.tonic, PitchClass::new(2));
        assert_eq!(p.style, "orchestral");
        assert_eq!(p.instruments[0], "violin");
        assert_eq!(p.sections, vec!['A', 'B', 'C']);
        assert!(p.energy > 0.8);
    }

    #[test]
    fn suite_has_open_middle_sections() {
        let s = to_suite(&plan("nopea valssi"));
        assert_eq!(s.section[0].meter, "3/4");
        assert!(s.section[0].ends_open);
        assert!(!s.section.last().unwrap().ends_open);
        assert!(s.section[0].chords.contains('|'));
    }

    #[test]
    fn transposition_of_progression() {
        assert_eq!(transpose_progression("Am Dm E7 | F", 2), "Bm Em F#7 | G");
    }
}
