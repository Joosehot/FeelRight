//! Pitch classes, chords, keys.

use std::fmt;

/// Pitch class 0..12, 0 = C.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PitchClass(pub u8);

impl PitchClass {
    pub fn new(n: i32) -> Self {
        PitchClass(n.rem_euclid(12) as u8)
    }
    pub fn add(self, semis: i32) -> Self {
        PitchClass::new(self.0 as i32 + semis)
    }
    /// Semitones from `self` up to `other` (0..12).
    pub fn interval_to(self, other: PitchClass) -> u8 {
        (other.0 as i32 - self.0 as i32).rem_euclid(12) as u8
    }
    pub fn of_midi(midi: u8) -> Self {
        PitchClass(midi % 12)
    }
    pub fn name(self) -> &'static str {
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        NAMES[self.0 as usize]
    }
}

impl fmt::Display for PitchClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    Maj,
    Min,
    Dim,
    Aug,
    Dom7,
    Maj7,
    Min7,
    HalfDim7,
    Sus2,
    Sus4,
}

impl Quality {
    /// Chord tones as semitone offsets from the root.
    pub fn intervals(self) -> &'static [i32] {
        match self {
            Quality::Maj => &[0, 4, 7],
            Quality::Min => &[0, 3, 7],
            Quality::Dim => &[0, 3, 6],
            Quality::Aug => &[0, 4, 8],
            Quality::Dom7 => &[0, 4, 7, 10],
            Quality::Maj7 => &[0, 4, 7, 11],
            Quality::Min7 => &[0, 3, 7, 10],
            Quality::HalfDim7 => &[0, 3, 6, 10],
            Quality::Sus2 => &[0, 2, 7],
            Quality::Sus4 => &[0, 5, 7],
        }
    }
    pub fn suffix(self) -> &'static str {
        match self {
            Quality::Maj => "",
            Quality::Min => "m",
            Quality::Dim => "dim",
            Quality::Aug => "aug",
            Quality::Dom7 => "7",
            Quality::Maj7 => "maj7",
            Quality::Min7 => "m7",
            Quality::HalfDim7 => "m7b5",
            Quality::Sus2 => "sus2",
            Quality::Sus4 => "sus4",
        }
    }
    pub fn has_seventh(self) -> bool {
        matches!(
            self,
            Quality::Dom7 | Quality::Maj7 | Quality::Min7 | Quality::HalfDim7
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub root: PitchClass,
    pub quality: Quality,
    pub tones: Vec<PitchClass>,
}

impl Chord {
    pub fn new(root: PitchClass, quality: Quality) -> Self {
        let tones = quality.intervals().iter().map(|&i| root.add(i)).collect();
        Chord { root, quality, tones }
    }
    pub fn contains(&self, pc: PitchClass) -> bool {
        self.tones.contains(&pc)
    }
    pub fn contains_midi(&self, midi: u8) -> bool {
        self.contains(PitchClass::of_midi(midi))
    }
    pub fn third(&self) -> PitchClass {
        self.tones[1]
    }
    pub fn fifth(&self) -> PitchClass {
        self.tones[2]
    }
    pub fn seventh(&self) -> Option<PitchClass> {
        self.tones.get(3).copied()
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.root, self.quality.suffix())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Major,
    Minor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub tonic: PitchClass,
    pub mode: Mode,
}

impl Key {
    pub fn new(tonic: PitchClass, mode: Mode) -> Self {
        Key { tonic, mode }
    }
    /// Scale as semitone offsets. Minor is natural minor; the leading tone
    /// is exposed separately via `leading_tone`.
    pub fn scale_intervals(&self) -> [i32; 7] {
        match self.mode {
            Mode::Major => [0, 2, 4, 5, 7, 9, 11],
            Mode::Minor => [0, 2, 3, 5, 7, 8, 10],
        }
    }
    pub fn scale(&self) -> Vec<PitchClass> {
        self.scale_intervals().iter().map(|&i| self.tonic.add(i)).collect()
    }
    /// Raised 7th (major 7th above tonic) in both modes.
    pub fn leading_tone(&self) -> PitchClass {
        self.tonic.add(11)
    }
    /// Scale degree 1..=7 for a pitch class, if diatonic (leading tone in
    /// minor counts as degree 7).
    pub fn degree(&self, pc: PitchClass) -> Option<u8> {
        let iv = self.tonic.interval_to(pc) as i32;
        if let Some(pos) = self.scale_intervals().iter().position(|&i| i == iv) {
            return Some(pos as u8 + 1);
        }
        if self.mode == Mode::Minor && iv == 11 {
            return Some(7);
        }
        None
    }
    pub fn is_diatonic(&self, pc: PitchClass) -> bool {
        self.degree(pc).is_some()
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = match self.mode {
            Mode::Major => "major",
            Mode::Minor => "minor",
        };
        write!(f, "{}:{}", self.tonic, m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_tones() {
        let c = Chord::new(PitchClass(0), Quality::Dom7);
        assert_eq!(
            c.tones,
            vec![PitchClass(0), PitchClass(4), PitchClass(7), PitchClass(10)]
        );
        assert!(c.contains_midi(64)); // E4
        assert!(!c.contains_midi(62)); // D4
    }

    #[test]
    fn key_degrees() {
        let k = Key::new(PitchClass(9), Mode::Minor); // A minor
        assert_eq!(k.degree(PitchClass(9)), Some(1));
        assert_eq!(k.degree(PitchClass(7)), Some(7)); // G natural
        assert_eq!(k.degree(PitchClass(8)), Some(7)); // G# leading tone
        assert_eq!(k.degree(PitchClass(10)), None);
        assert_eq!(k.leading_tone(), PitchClass(8));
    }
}
