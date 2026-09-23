//! MIDI writer: melody track (with dynamics), accompaniment track, bass track.

use crate::model::{Melody, Meter, Note, Style, PPQ, TICKS_PER_STEP};
use crate::parser::ChordSpan;
use anyhow::{Context, Result};
use midly::{
    num::{u15, u24, u28, u4, u7},
    Format, Header, MetaMessage, MidiMessage, Smf, Timing, Track, TrackEvent, TrackEventKind,
};
use std::path::Path;

struct AbsEvent {
    tick: u32,
    /// Note-offs sort before note-ons at the same tick.
    order: u8,
    kind: TrackEventKind<'static>,
}

fn to_track(mut abs: Vec<AbsEvent>) -> Track<'static> {
    abs.sort_by_key(|e| (e.tick, e.order));
    let mut track = Track::new();
    let mut last = 0u32;
    for e in abs {
        track.push(TrackEvent {
            delta: u28::new(e.tick - last),
            kind: e.kind,
        });
        last = e.tick;
    }
    track.push(TrackEvent {
        delta: u28::new(0),
        kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
    });
    track
}

/// Swing: off-beat 8ths (grid step 2 of a beat) are played late by a
/// third of a beat, so a pair of 8ths becomes a triplet feel.
const SWING_TICKS: u32 = 80;

fn swing_tick(step: u32, swing: bool) -> u32 {
    let mut t = step * TICKS_PER_STEP;
    if swing && step % 4 == 2 {
        t += SWING_TICKS;
    }
    t
}

fn note_pair_swing(abs: &mut Vec<AbsEvent>, channel: u8, pitch: u8, vel: u8, start: u32, dur: u32, swing: bool) {
    let ch = u4::new(channel);
    let on = swing_tick(start, swing);
    let off = swing_tick(start + dur, swing).max(on + 1);
    abs.push(AbsEvent {
        tick: on,
        order: 1,
        kind: TrackEventKind::Midi { channel: ch, message: MidiMessage::NoteOn { key: u7::new(pitch), vel: u7::new(vel) } },
    });
    abs.push(AbsEvent {
        tick: off,
        order: 0,
        kind: TrackEventKind::Midi { channel: ch, message: MidiMessage::NoteOff { key: u7::new(pitch), vel: u7::new(0) } },
    });
}

fn note_pair(abs: &mut Vec<AbsEvent>, channel: u8, pitch: u8, vel: u8, start: u32, dur: u32) {
    let ch = u4::new(channel);
    abs.push(AbsEvent {
        tick: start * TICKS_PER_STEP,
        order: 1,
        kind: TrackEventKind::Midi {
            channel: ch,
            message: MidiMessage::NoteOn {
                key: u7::new(pitch),
                vel: u7::new(vel),
            },
        },
    });
    abs.push(AbsEvent {
        tick: (start + dur) * TICKS_PER_STEP,
        order: 0,
        kind: TrackEventKind::Midi {
            channel: ch,
            message: MidiMessage::NoteOff {
                key: u7::new(pitch),
                vel: u7::new(0),
            },
        },
    });
}

/// Voicing for the block-chord track: root in octave 3, tones stacked above.
fn chord_voicing(span: &ChordSpan) -> Vec<u8> {
    let root = 48 + span.chord.root.0; // C3 = 48
    span.chord
        .tones
        .iter()
        .map(|pc| root + span.chord.root.interval_to(*pc))
        .collect()
}

/// Start and end step of the phrase containing `bar`.
fn phrase_bounds(bar: u32, phrase_ends: &[u32], spb: u32) -> (u32, u32) {
    let end_bar = phrase_ends.iter().copied().find(|&e| e >= bar).unwrap_or(bar);
    let start_bar = phrase_ends.iter().copied().filter(|&e| e < bar).max().map(|e| e + 1).unwrap_or(0);
    (start_bar * spb, (end_bar + 1) * spb)
}

/// The chord that follows `span`, if any.
fn chords_after<'a>(chords: &'a [ChordSpan], span: &ChordSpan) -> Option<&'a crate::theory::Chord> {
    chords.iter().find(|s| s.start == span.start + span.len).map(|s| &s.chord)
}

/// Alberti-bass note length: one 8th.
const ALBERTI_STEP: u32 = 2;
const MEL_VEL_LO: u8 = 66;
const MEL_VEL_HI: u8 = 118;
const BASS_VEL_LO: u8 = 56;
const BASS_VEL_HI: u8 = 92;

/// Broken-chord accompaniment: low, high, middle, high (root, 5th, 3rd, 5th)
/// in 8ths, repeated for the length of the chord. Four-note chords use the
/// 7th as the top voice instead of the 5th.
fn alberti(span: &ChordSpan) -> Vec<(u32, u8)> {
    let v = chord_voicing(span);
    let (low, mid, high) = match v.len() {
        4 => (v[0], v[1], v[3]),
        _ => (v[0], v[1], v[2]),
    };
    let cycle = [low, high, mid, high];
    let mut out = Vec::new();
    let mut t = span.start;
    let mut i = 0;
    while t + ALBERTI_STEP <= span.start + span.len {
        out.push((t, cycle[i % 4]));
        t += ALBERTI_STEP;
        i += 1;
    }
    out
}

/// Voice-led voicing: pick the inversion (any octave placement within
/// `lo..=hi`, ascending) whose notes move least from `prev`.
fn voice_lead(span: &ChordSpan, prev: Option<&[u8]>, lo: u8, hi: u8) -> Vec<u8> {
    let tones = &span.chord.tones;
    let n = tones.len();
    let mut best: Option<(i32, Vec<u8>)> = None;
    for rot in 0..n {
        for base_oct in 0..3u8 {
            // Lowest note of this inversion placed in the octave above lo.
            let first_pc = tones[rot].0;
            let mut low = lo - lo % 12 + first_pc + 12 * base_oct;
            while low < lo {
                low += 12;
            }
            if low > hi {
                continue;
            }
            let mut v = vec![low];
            for k in 1..n {
                let pc = tones[(rot + k) % n].0;
                let mut p = v[k - 1] + ((pc as i32 - v[k - 1] as i32).rem_euclid(12)) as u8;
                if p == v[k - 1] {
                    p += 12;
                }
                v.push(p);
            }
            if *v.last().unwrap() > hi + 7 {
                continue;
            }
            let cost = match prev {
                Some(pv) => {
                    let m = v.len().min(pv.len());
                    (0..m).map(|i| (v[i] as i32 - pv[i] as i32).abs()).sum::<i32>()
                        + (v[0] as i32 - lo as i32 - 7).abs() / 4
                }
                None => (v[0] as i32 - lo as i32 - 5).abs(),
            };
            if best.as_ref().map(|b| cost < b.0).unwrap_or(true) {
                best = Some((cost, v));
            }
        }
    }
    best.map(|b| b.1).unwrap_or_else(|| chord_voicing(span))
}

/// Alberti figure over a given voicing: low, high, mid, high.
fn alberti_on(v: &[u8], start: u32, len: u32, step: u32) -> Vec<(u32, u8)> {
    let (low, mid, high) = match v.len() {
        4 => (v[0], v[1], v[3]),
        _ => (v[0], v[1], v[2]),
    };
    let cycle = [low, high, mid, high];
    let mut out = Vec::new();
    let mut t = start;
    let mut i = 0;
    while t + step <= start + len {
        out.push((t, cycle[i % 4]));
        t += step;
        i += 1;
    }
    out
}

/// One section of a piece, rendered at `offset` grid steps.
pub struct Section<'a> {
    pub melody: &'a Melody,
    pub chords: &'a [ChordSpan],
    pub meter: Meter,
    pub tempo_bpm: u32,
    pub style: Style,
    pub energy: &'a [f32],
    pub program: u8,
    pub octave: i32,
    pub phrase_ends: &'a [u32],
    pub offset: u32,
}

const CH_CHORDS: u8 = 8;
const CH_BASS: u8 = 10;
const CH_LAYER: u8 = 11;
const GM_STRINGS: u8 = 48;
const GM_CONTRABASS: u8 = 43;
const GM_CELLO: u8 = 42;
const GM_BRASS_SECTION: u8 = 61;
const GM_EPIANO: u8 = 4;
const GM_ACOUSTIC_BASS: u8 = 32;
const CH_DRUMS: u8 = 9;
const DRUM_RIDE: u8 = 51;
const DRUM_HIHAT_PEDAL: u8 = 44;
const GM_TUBA: u8 = 58;
const GM_TIMPANI: u8 = 47;

fn program_change(abs: &mut Vec<AbsEvent>, tick: u32, channel: u8, program: u8) {
    abs.push(AbsEvent {
        tick,
        order: 0,
        kind: TrackEventKind::Midi {
            channel: u4::new(channel),
            message: MidiMessage::ProgramChange { program: u7::new(program.min(127)) },
        },
    });
}

/// Write one or more sections into a single MIDI file. Each section gets
/// its own melody track and channel so instruments can differ; tempo and
/// time signature changes go on the conductor track.
pub fn write_suite(path: &Path, sections: &[Section]) -> Result<()> {
    let header = Header::new(Format::Parallel, Timing::Metrical(u15::new(PPQ as u16)));
    let mut smf = Smf::new(header);

    let mut cond = Vec::new();
    let mut chd = Vec::new();
    let mut bass = Vec::new();
    let mut layer = Vec::new();
    let mut melody_tracks: Vec<Vec<AbsEvent>> = Vec::new();

    for (si, sec) in sections.iter().enumerate() {
        let off = sec.offset;
        let spb = sec.meter.steps_per_bar();
        let tick0 = off * TICKS_PER_STEP;
        cond.push(AbsEvent {
            tick: tick0,
            order: 0,
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(60_000_000 / sec.tempo_bpm.max(1)))),
        });
        cond.push(AbsEvent {
            tick: tick0,
            order: 0,
            kind: TrackEventKind::Meta(MetaMessage::TimeSignature(sec.meter.num, sec.meter.den.trailing_zeros() as u8, 24, 8)),
        });

        // Melody: channel = section index, skipping the drum channel and
        // the accompaniment channels.
        let ch = [0u8, 1, 2, 3, 4, 5, 6, 7, 12, 13, 14, 15][si % 12];
        let mut mel = Vec::new();
        program_change(&mut mel, tick0, ch, sec.program);
        let notes: Vec<Note> = sec.melody.notes().copied().collect();
        for (i, n) in notes.iter().enumerate() {
            let bar = n.start / spb;
            let e = sec.energy.get(bar as usize).copied().unwrap_or(0.5);
            let base = MEL_VEL_LO as f32 + (MEL_VEL_HI - MEL_VEL_LO) as f32 * e;
            let (p_start, p_end) = phrase_bounds(bar, sec.phrase_ends, spb);
            let pos = (n.start - p_start) as f32 / (p_end - p_start).max(1) as f32;
            let arc = if pos < 0.6 { 0.85 + 0.15 * pos / 0.6 } else { 1.0 - 0.2 * (pos - 0.6) / 0.4 };
            let accent = match sec.meter.strength(n.start % spb) {
                s if s >= 1.0 => 8.0,
                s if s >= 0.75 => 4.0,
                s if s >= 0.5 => 0.0,
                _ => -4.0,
            };
            let vel = (base * arc + accent).round().clamp(1.0, 127.0) as u8;
            let is_phrase_final = sec.phrase_ends.contains(&bar)
                && notes.get(i + 1).map(|q| q.start / spb != bar).unwrap_or(true);
            let stepwise = i > 0 && (n.pitch as i32 - notes[i - 1].pitch as i32).abs() <= 2;
            let dur = if is_phrase_final {
                n.dur - (n.dur / 4).clamp(1, 4)
            } else if stepwise && n.dur <= 2 {
                n.dur
            } else {
                (n.dur as f32 * 0.9).round().max(1.0) as u32
            };
            let pitch = (n.pitch as i32 + 12 * sec.octave).clamp(0, 127) as u8;
            note_pair_swing(&mut mel, ch, pitch, vel, off + n.start, dur, sec.style == Style::Jazz);
        }
        melody_tracks.push(mel);

        // Accompaniment.
        let (chord_prog, bass_prog, layer_prog) = match sec.style {
            Style::Orchestral => (GM_STRINGS, GM_CONTRABASS, GM_CELLO),
            Style::Brass => (GM_BRASS_SECTION, GM_TUBA, GM_TIMPANI),
            Style::Concerto => (0, GM_CONTRABASS, GM_STRINGS),
            Style::Waltz => (0, GM_CONTRABASS, GM_STRINGS),
            Style::Rapids => (0, 0, GM_STRINGS),
            Style::Jazz => (GM_EPIANO, GM_ACOUSTIC_BASS, 0),
            _ => (0, 0, 0),
        };
        program_change(&mut chd, tick0, CH_CHORDS, chord_prog);
        program_change(&mut bass, tick0, CH_BASS, bass_prog);
        program_change(&mut layer, tick0, CH_LAYER, layer_prog);
        let mut prev_voicing: Option<Vec<u8>> = None;
        for span in sec.chords {
            let e = sec.energy.get((span.start / spb) as usize).copied().unwrap_or(0.5);
            let root = 36 + span.chord.root.0;
            let bvel = (BASS_VEL_LO as f32 + (BASS_VEL_HI - BASS_VEL_LO) as f32 * e).round() as u8;
            let bar = span.start / spb;
            let phrase_end_bar = sec.phrase_ends.contains(&bar);
            match sec.style {
                Style::Pop => {
                    for p in chord_voicing(span) {
                        note_pair(&mut chd, CH_CHORDS, p, 60, off + span.start, span.len);
                    }
                    note_pair(&mut bass, CH_BASS, root, bvel, off + span.start, span.len);
                }
                Style::Classical => {
                    // Voice-led left hand in the small octave; texture by
                    // tension: calm = held chord, mid = Alberti 8ths,
                    // tense = Alberti 16ths. Phrase-final bars hold.
                    let v = voice_lead(span, prev_voicing.as_deref(), 48, 60);
                    let vel = (40.0 + 36.0 * e).round() as u8;
                    let spbeat = sec.meter.steps_per_beat();
                    let end = span.start + span.len;
                    if phrase_end_bar || e < 0.3 {
                        for p in &v {
                            note_pair(&mut chd, CH_CHORDS, *p, vel, off + span.start, span.len);
                        }
                        note_pair(&mut bass, CH_BASS, root, bvel, off + span.start, span.len);
                    } else {
                        let step = if e >= 0.75 { 1 } else { ALBERTI_STEP };
                        for (start, p) in alberti_on(&v, span.start, span.len, step) {
                            let accent = if (start - span.start) % spbeat == 0 { 6 } else { 0 };
                            note_pair(&mut chd, CH_CHORDS, p, vel + accent, off + start, step);
                        }
                        // Classical bass: root on strong beats, fifth on
                        // the others, in half notes.
                        let fifth = root + 7;
                        let mut t = span.start;
                        let mut i = 0;
                        while t < end {
                            let len = (2 * spbeat).min(end - t);
                            let p = if i % 2 == 0 { root } else { fifth.min(root + 7) };
                            note_pair(&mut bass, CH_BASS, p, bvel, off + t, len);
                            t += len;
                            i += 1;
                        }
                    }
                    prev_voicing = Some(v);
                }
                Style::Brass => {
                    // Texture follows the tension of the bar:
                    //   e < 0.4  sustained chord, tuba on the downbeat
                    //   e < 0.7  half-note chords, tuba on strong beats
                    //   else     beat hits + off-beat stabs, oom-pah tuba, timpani
                    let spbeat = sec.meter.steps_per_beat();
                    let end = span.start + span.len;
                    let tuba = root.saturating_sub(12).max(24);
                    let fifth = tuba + 7;
                    let vel = (48.0 + 44.0 * e).round() as u8;
                    if e < 0.4 {
                        for p in chord_voicing(span) {
                            note_pair(&mut chd, CH_CHORDS, p + 12, vel, off + span.start, span.len);
                        }
                        note_pair(&mut bass, CH_BASS, tuba, bvel, off + span.start, span.len);
                    } else if e < 0.7 {
                        let mut t = span.start;
                        while t < end {
                            if sec.meter.is_strong(t % spb) {
                                let len = (2 * spbeat).min(end - t);
                                for p in chord_voicing(span) {
                                    note_pair(&mut chd, CH_CHORDS, p + 12, vel, off + t, len);
                                }
                                note_pair(&mut bass, CH_BASS, tuba, bvel, off + t, len);
                                if t % spb == 0 {
                                    note_pair(&mut layer, CH_LAYER, root, (50.0 + 40.0 * e).round() as u8, off + t, spbeat);
                                }
                            }
                            t += spbeat;
                        }
                    } else {
                        let mut t = span.start;
                        while t < end {
                            let pos = t % spb;
                            let strong = sec.meter.is_strong(pos);
                            let len = if strong { spbeat.min(end - t) } else { spbeat / 2 };
                            let v = vel + if strong { 10 } else { 0 };
                            for p in chord_voicing(span) {
                                note_pair(&mut chd, CH_CHORDS, p + 12, v, off + t, len);
                            }
                            // Off-beat stab on the last beat of the bar.
                            if pos + spbeat == spb {
                                for p in chord_voicing(span) {
                                    note_pair(&mut chd, CH_CHORDS, p + 12, v.saturating_sub(12), off + t + spbeat / 2, spbeat / 4);
                                }
                            }
                            // Oom-pah: root on strong beats, fifth on weak.
                            note_pair(&mut bass, CH_BASS, if strong { tuba } else { fifth }, bvel, off + t, spbeat / 2);
                            if strong {
                                note_pair(&mut layer, CH_LAYER, root, (60.0 + 50.0 * e).round() as u8, off + t, spbeat);
                            } else if e > 0.85 && pos + spbeat == spb {
                                for k in 0..4 {
                                    note_pair(&mut layer, CH_LAYER, root, 70, off + t + k, 1);
                                }
                            }
                            t += spbeat;
                        }
                    }
                }
                Style::Jazz => {
                    let spbeat = sec.meter.steps_per_beat();
                    let end = span.start + span.len;
                    let v = chord_voicing(span);
                    // Comping (Charleston: beat 1 long, "and" of 2 short;
                    // an extra push before beat 4 when tense), rootless
                    // voicing an octave up.
                    let mut t = span.start;
                    while t < end {
                        let pos = t % spb;
                        let vel = (40.0 + 40.0 * e).round() as u8;
                        let hits: &[(u32, u32)] = if e >= 0.6 { &[(0, 3), (6, 2), (10, 2)] } else { &[(0, 3), (6, 2)] };
                        for &(at, len) in hits {
                            if pos == 0 && t + at < end {
                                for p in v.iter().skip(1) {
                                    note_pair_swing(&mut chd, CH_CHORDS, p + 12, vel, off + t + at, len, true);
                                }
                            }
                        }
                        t += spb - pos;
                    }
                    // Walking bass: root, 3rd, 5th, then a chromatic
                    // approach to the next root (or the 5th again).
                    let next_root = chords_after(sec.chords, span).map(|c| 36 + c.root.0);
                    let walk = [v[0], v[1], v[2], next_root.map(|r| if r > v[0] { r - 1 } else { r + 1 }).unwrap_or(v[2])];
                    let mut t = span.start;
                    let mut i = 0;
                    while t < end {
                        let p = walk[i % 4].saturating_sub(12).max(28);
                        note_pair(&mut bass, CH_BASS, p, bvel, off + t, spbeat);
                        t += spbeat;
                        i += 1;
                    }
                    // Ride on every beat with the swung skip note on 2 and
                    // 4; pedal hi-hat on 2 and 4.
                    let mut t = span.start;
                    while t < end {
                        let pos = t % spb;
                        let beat = pos / spbeat;
                        let rv = (60.0 + 30.0 * e).round() as u8;
                        note_pair(&mut layer, CH_DRUMS, DRUM_RIDE, rv, off + t, 1);
                        if beat % 2 == 1 {
                            note_pair_swing(&mut layer, CH_DRUMS, DRUM_RIDE, rv.saturating_sub(15), off + t + 2, 1, true);
                            note_pair(&mut layer, CH_DRUMS, DRUM_HIHAT_PEDAL, 70, off + t, 1);
                        }
                        t += spbeat;
                    }
                }
                Style::Rapids => {
                    // Left hand: 16th arpeggio up two octaves and back,
                    // 8 notes per half bar, velocity rising and falling
                    // with the figure. Octave bass on beat 1 of each span.
                    // Faster (32nd-like doubling) when tension is high.
                    let v = chord_voicing(span);
                    let (a, b, c) = (v[0], v[1], v[2]);
                    let cycle = [a, b, c, a + 12, b + 12, c + 12, b + 12, c];
                    let end = span.start + span.len;
                    let mut t = span.start;
                    let mut i = 0usize;
                    let base_vel = 34.0 + 40.0 * e;
                    while t < end {
                        let shape = 1.0 + 0.25 * (i % 8) as f32 / 8.0;
                        let vel = (base_vel * shape).round().min(110.0) as u8;
                        note_pair(&mut chd, CH_CHORDS, cycle[i % 8], vel, off + t, 1);
                        t += 1;
                        i += 1;
                    }
                    let bvel = (BASS_VEL_LO as f32 + (BASS_VEL_HI - BASS_VEL_LO) as f32 * e).round() as u8;
                    note_pair(&mut bass, CH_BASS, root.saturating_sub(12).max(24), bvel, off + span.start, span.len);
                    note_pair(&mut bass, CH_BASS, root, bvel, off + span.start, span.len);
                    if e >= 0.6 {
                        let svel = (20.0 + 30.0 * e).round() as u8;
                        for p in chord_voicing(span) {
                            note_pair(&mut layer, CH_LAYER, p + 12, svel, off + span.start, span.len);
                        }
                    }
                }
                Style::Waltz => {
                    // Bass note on beat 1, chord on the other beats, both
                    // in the piano; strings hold the chord when tense.
                    let spbeat = sec.meter.steps_per_beat();
                    let end = span.start + span.len;
                    let mut t = span.start;
                    let vel = (46.0 + 40.0 * e).round() as u8;
                    while t < end {
                        let pos = t % spb;
                        if pos == 0 {
                            note_pair(&mut chd, CH_CHORDS, root, vel + 10, off + t, spbeat);
                            note_pair(&mut bass, CH_BASS, root, bvel, off + t, (spb).min(end - t));
                        } else {
                            for p in chord_voicing(span) {
                                note_pair(&mut chd, CH_CHORDS, p + 12, vel.saturating_sub(6), off + t, spbeat / 2);
                            }
                        }
                        t += spbeat;
                    }
                    if e >= 0.4 {
                        let svel = (30.0 + 40.0 * e).round() as u8;
                        for p in chord_voicing(span) {
                            note_pair(&mut layer, CH_LAYER, p + 12, svel, off + span.start, span.len);
                        }
                    }
                }
                Style::Concerto => {
                    // Piano: full chord in two octaves on strong beats, a
                    // lighter repeat on weak beats when tense. Strings hold
                    // the chord; contrabass holds the root.
                    let spbeat = sec.meter.steps_per_beat();
                    let end = span.start + span.len;
                    let mut t = span.start;
                    while t < end {
                        let pos = t % spb;
                        let strong = sec.meter.is_strong(pos);
                        if strong || e >= 0.6 {
                            let len = if strong { (2 * spbeat).min(end - t) } else { spbeat / 2 };
                            let vel = (52.0 + 48.0 * e + if strong { 8.0 } else { -8.0 }).round().min(127.0) as u8;
                            for p in chord_voicing(span) {
                                note_pair(&mut chd, CH_CHORDS, p, vel, off + t, len);
                                note_pair(&mut chd, CH_CHORDS, p + 12, vel, off + t, len);
                            }
                            // Octave bass in the left hand on strong beats.
                            if strong {
                                note_pair(&mut chd, CH_CHORDS, root.saturating_sub(12).max(24), vel, off + t, len);
                            }
                        }
                        t += spbeat;
                    }
                    let svel = (36.0 + 40.0 * e).round() as u8;
                    for p in chord_voicing(span) {
                        note_pair(&mut layer, CH_LAYER, p + 12, svel, off + span.start, span.len);
                    }
                    note_pair(&mut bass, CH_BASS, root, bvel, off + span.start, span.len);
                }
                Style::Orchestral => {
                    // Sustained string chord, voiced an octave up from the
                    // block voicing so it sits under the melody.
                    let vel = (40.0 + 40.0 * e).round() as u8;
                    let v = voice_lead(span, prev_voicing.as_deref(), 55, 67);
                    for p in &v {
                        note_pair(&mut chd, CH_CHORDS, *p, vel, off + span.start, span.len);
                    }
                    prev_voicing = Some(v);
                    // Contrabass: root on every strong beat.
                    let spbeat = sec.meter.steps_per_beat();
                    let mut t = span.start;
                    while t < span.start + span.len {
                        let pos = t % spb;
                        if sec.meter.is_strong(pos) {
                            let len = (2 * spbeat).min(span.start + span.len - t);
                            note_pair(&mut bass, CH_BASS, root, bvel, off + t, len);
                        }
                        t += spbeat;
                    }
                    // Cello layer: Alberti figure, quieter, for motion.
                    let lvel = (30.0 + 30.0 * e).round() as u8;
                    for (start, p) in alberti(span) {
                        note_pair(&mut layer, CH_LAYER, p, lvel, off + start, ALBERTI_STEP);
                    }
                }
            }
        }
    }

    smf.tracks.push(to_track(cond));
    for mel in melody_tracks {
        smf.tracks.push(to_track(mel));
    }
    smf.tracks.push(to_track(chd));
    smf.tracks.push(to_track(bass));
    if sections.iter().any(|s| matches!(s.style, Style::Orchestral | Style::Brass | Style::Concerto | Style::Waltz | Style::Rapids | Style::Jazz)) {
        smf.tracks.push(to_track(layer));
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    smf.save(path)
        .with_context(|| format!("writing MIDI to {}", path.display()))?;
    Ok(())
}

pub fn write_midi(
    path: &Path,
    melody: &Melody,
    chords: &[ChordSpan],
    meter: Meter,
    tempo_bpm: u32,
    style: Style,
    energy: &[f32],
    program: u8,
    octave: i32,
    phrase_ends: &[u32],
) -> Result<()> {
    write_suite(
        path,
        &[Section { melody, chords, meter, tempo_bpm, style, energy, program, octave, phrase_ends, offset: 0 }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Event, Note};
    use crate::parser::parse_progression;

    #[test]
    fn roundtrip_smoke() {
        let meter = Meter { num: 4, den: 4 };
        let chords = parse_progression("C G", meter, 2).unwrap();
        let mut m = Melody::default();
        m.push(Event::Note(Note { pitch: 60, start: 0, dur: 8 }));
        m.push(Event::Rest { start: 8, dur: 8 });
        m.push(Event::Note(Note { pitch: 67, start: 16, dur: 16 }));
        let path = std::env::temp_dir().join("melody_test").join("smoke.mid");
        write_midi(&path, &m, &chords, meter, 120, Style::Pop, &[0.5, 0.5], 0, 0, &[1]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let smf = Smf::parse(&bytes).unwrap();
        assert_eq!(smf.tracks.len(), 4);
        let note_ons = smf.tracks[1]
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    TrackEventKind::Midi { message: MidiMessage::NoteOn { .. }, .. }
                )
            })
            .count();
        assert_eq!(note_ons, 2);
        // Chord track: C (3 tones) + G (3 tones)
        let chord_ons = smf.tracks[2]
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    TrackEventKind::Midi { message: MidiMessage::NoteOn { .. }, .. }
                )
            })
            .count();
        assert_eq!(chord_ons, 6);
    }

    #[test]
    fn alberti_fills_span() {
        let meter = Meter { num: 4, den: 4 };
        let chords = parse_progression("C G7", meter, 2).unwrap();
        let notes = alberti(&chords[0]);
        assert_eq!(notes.len(), 8);
        assert_eq!(notes[0], (0, 48)); // C3
        assert_eq!(notes[1].1, 55); // G3
        assert_eq!(notes[2].1, 52); // E3
        let notes = alberti(&chords[1]);
        assert_eq!(notes[1].1, 55 + 10); // F4, the 7th of G7
    }
}
