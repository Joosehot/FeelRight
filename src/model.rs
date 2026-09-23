//! Time grid, notes, melody.

use crate::parser::ChordSpan;
use crate::theory::Chord;

/// Grid steps per quarter note (16th-note grid).
pub const STEPS_PER_QUARTER: u32 = 4;
/// MIDI pulses per quarter note.
pub const PPQ: u32 = 480;
/// MIDI ticks per grid step.
pub const TICKS_PER_STEP: u32 = PPQ / STEPS_PER_QUARTER;

/// Stylistic preset: selects the rhythm library and the accompaniment
/// texture. The rules themselves are style-neutral.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Style {
    /// Even and dotted rhythms, pickups, broken-chord (Alberti) accompaniment.
    #[default]
    Classical,
    /// Syncopated 8th-note rhythms, block-chord accompaniment.
    Pop,
    /// Classical rhythms; sustained string chords, contrabass, and a
    /// second string layer in the accompaniment.
    Orchestral,
    /// Classical rhythms; brass-section chord hits on the beats, tuba
    /// bass, timpani on strong beats.
    Brass,
    /// Classical rhythms; grand piano chords in octaves on strong beats,
    /// sustained strings, contrabass.
    Concerto,
    /// Classical rhythms; oom-pah-pah piano (bass on 1, chords on 2 and
    /// 3), sustained strings. Meant for 3/4.
    Waltz,
    /// Classical rhythms; flowing 16th-note piano arpeggios over two
    /// octaves with an octave bass: rapids.
    Rapids,
    /// Syncopated rhythms with swing; walking bass, Charleston comping,
    /// ride cymbal and hi-hat.
    Jazz,
    /// Comic oom-pah: tuba on 1 and 3, chord stabs on 2 and 4, woodblock,
    /// staccato melody.
    Circus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meter {
    pub num: u8,
    pub den: u8,
}

impl Meter {
    pub fn steps_per_beat(&self) -> u32 {
        STEPS_PER_QUARTER * 4 / self.den as u32
    }
    pub fn steps_per_bar(&self) -> u32 {
        self.steps_per_beat() * self.num as u32
    }
    /// Beat strength 0..1 for a position inside a bar (in steps).
    /// Downbeat 1.0, other strong beats 0.75, weak beats 0.5,
    /// off-beat 8ths 0.25, 16ths 0.1.
    pub fn strength(&self, pos_in_bar: u32) -> f32 {
        let spb = self.steps_per_beat();
        if pos_in_bar == 0 {
            return 1.0;
        }
        if pos_in_bar % spb == 0 {
            let beat = pos_in_bar / spb;
            let strong = match self.num {
                4 => beat == 2,
                6 | 9 | 12 => beat % 3 == 0,
                _ => false,
            };
            return if strong { 0.75 } else { 0.5 };
        }
        if pos_in_bar % (spb / 2) == 0 {
            return 0.25;
        }
        0.1
    }
    pub fn is_strong(&self, pos_in_bar: u32) -> bool {
        self.strength(pos_in_bar) >= 0.75
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Note {
    pub pitch: u8,
    /// Start in grid steps from the beginning of the piece.
    pub start: u32,
    /// Duration in grid steps.
    pub dur: u32,
}

impl Note {
    pub fn end(&self) -> u32 {
        self.start + self.dur
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Note(Note),
    Rest { start: u32, dur: u32 },
}

impl Event {
    pub fn start(&self) -> u32 {
        match self {
            Event::Note(n) => n.start,
            Event::Rest { start, .. } => *start,
        }
    }
    pub fn dur(&self) -> u32 {
        match self {
            Event::Note(n) => n.dur,
            Event::Rest { dur, .. } => *dur,
        }
    }
    pub fn end(&self) -> u32 {
        self.start() + self.dur()
    }
    pub fn note(&self) -> Option<&Note> {
        match self {
            Event::Note(n) => Some(n),
            Event::Rest { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Melody {
    pub events: Vec<Event>,
}

impl Melody {
    pub fn notes(&self) -> impl Iterator<Item = &Note> {
        self.events.iter().filter_map(Event::note)
    }
    pub fn push(&mut self, e: Event) {
        self.events.push(e);
    }
    pub fn total_steps(&self) -> u32 {
        self.events.last().map(Event::end).unwrap_or(0)
    }
    /// Events whose start falls inside bar `bar`.
    pub fn bar_events(&self, bar: u32, meter: Meter) -> Vec<Event> {
        let spb = meter.steps_per_bar();
        let (a, b) = (bar * spb, (bar + 1) * spb);
        self.events
            .iter()
            .copied()
            .filter(|e| e.start() >= a && e.start() < b)
            .collect()
    }
}

/// Look up which chord sounds at a given grid step.
pub fn chord_at(spans: &[ChordSpan], step: u32) -> Option<&Chord> {
    spans
        .iter()
        .find(|s| step >= s.start && step < s.start + s.len)
        .map(|s| &s.chord)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_grid() {
        let m = Meter { num: 4, den: 4 };
        assert_eq!(m.steps_per_bar(), 16);
        assert_eq!(m.strength(0), 1.0);
        assert_eq!(m.strength(8), 0.75);
        assert_eq!(m.strength(4), 0.5);
        assert_eq!(m.strength(2), 0.25);
        assert_eq!(m.strength(1), 0.1);
        let m = Meter { num: 6, den: 8 };
        assert_eq!(m.steps_per_bar(), 12);
        assert_eq!(m.strength(6), 0.75);
    }
}
