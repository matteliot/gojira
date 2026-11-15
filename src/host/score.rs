//! The `cargo sim` demo: its score (the rendering itself is `pedalkit::host`).
//!
//! Plays original drop-C riffs at 120 bpm on a modelled electric guitar (see
//! `guitar`: exact tuning, pick and pickup, strums, human timing), drives the
//! patch's entry points the way the firmware does (128-sample blocks,
//! footswitch events at set times) and writes a stereo float WAV: Magma
//! (ringing clean arpeggios into the chorus, echo and room) -> a pick scrape
//! -> Sirius (palm-muted chugs, power chords, staccato stops and a pinch
//! harmonic squeal) -> Whammy armed (a single-note riff, every note swooping
//! up an octave) -> staccato hits and a last chord left to ring out.
//! Prints per-section level stats and fails on NaN or clipping.

use crate::pedalkit::guitar::Note;
use crate::pedalkit::host::{Ev, Score};

// Drop C (C2 G2 C3 F3 A3 D4). Clean voicings, low string to high.
const CM9: [f32; 6] = [65.41, 98.0, 130.81, 196.0, 293.66, 311.13];
const ABMAJ7: [f32; 5] = [103.83, 155.56, 196.0, 261.63, 311.13];
const BB: [f32; 5] = [116.54, 174.61, 233.08, 293.66, 349.23];
const FSUS2: [f32; 5] = [87.31, 130.81, 196.0, 261.63, 392.0];
// Power chords (one-finger shapes on the bottom three strings).
const C5: [f32; 3] = [65.41, 98.0, 130.81];
const EB5: [f32; 3] = [77.78, 116.54, 155.56];
const F5: [f32; 3] = [87.31, 130.81, 174.61];
const GB5: [f32; 3] = [92.50, 138.59, 185.0];
const LOW_C: f32 = 65.41;

enum Local {
    /// Play these notes, strummed this many seconds apart; `ring` lets what's already sounding carry on.
    Play(Vec<Note>, f32, bool),
    /// A pick scrape down the wound strings, lasting this many seconds.
    Scrape(f32),
    Press,
    Hold,
}

fn chord(c: &[f32], vel: f32) -> Vec<Note> {
    c.iter().map(|f| Note::new(*f).vel(vel)).collect()
}

/// One sixteenth-note step of a riff: `x` palm-muted low C, `C E F T` power
/// chords (C5, Eb5, F5, Gb5), `h` a pinch-harmonic squeal, `.` a dead stop,
/// `-` let it ring.
fn step(ch: char) -> Option<Local> {
    let pc = |c: &[f32]| Local::Play(chord(c, 0.95), 0.004, false);
    Some(match ch {
        'x' => Local::Play(vec![Note::new(LOW_C).pm().vel(0.95)], 0.0, false),
        'C' => pc(&C5),
        'E' => pc(&EB5),
        'F' => pc(&F5),
        'T' => pc(&GB5),
        // A pinch harmonic on the G string: a high partial, bent up and shaken.
        'h' => Local::Play(vec![Note::new(784.0).vel(1.0).t60(1.5).bend(0.06, 0.12, 100.0).vib()], 0.0, false),
        '.' => Local::Play(vec![], 0.0, false),
        _ => return None,
    })
}

fn local_score() -> (Vec<(f64, Local, &'static str)>, f64) {
    let mut ev = vec![];
    let q = 60.0 / 120.0; // quarter note
    let e = q / 2.0;
    let s = q / 4.0;
    let mut t = 0.1;
    // Small, repeatable timing slop: nobody plays exactly on the grid.
    let mut seed: u32 = 0x1357_9bdf;
    let mut slop = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) * 0.004
    };

    // Magma: eighth-note arpeggios, a bar per chord, twice round.
    ev.push((0.0, Local::Press, ""));
    let prog: [&[f32]; 4] = [&CM9, &ABMAJ7, &BB, &FSUS2];
    for _ in 0..2 {
        for c in prog {
            let top = c.len() - 1;
            for (k, i) in [0, 1, 2, 3, top, 3, 2, 1].into_iter().enumerate() {
                let vel = if k == 0 { 0.7 } else { 0.55 };
                ev.push((t + slop(), Local::Play(vec![Note::new(c[i]).vel(vel)], 0.0, k != 0), ""));
                t += e;
            }
        }
    }

    // Sirius: a pick scrape leads in, then the chug riff, twice round.
    ev.push((t, Local::Press, "sirius: pick scrape"));
    ev.push((t, Local::Play(vec![], 0.0, false), ""));
    ev.push((t + 0.02, Local::Scrape((2.0 * q - 0.06) as f32), ""));
    t += 2.0 * q;
    let riff = ["x.xxx.xxC---.xx.", "x.xxx.xxE---.F--", "x.xxx.xxC---.xx.", "T---F---h-------"];
    ev.push((t, Local::Play(vec![], 0.0, true), "sirius (riff)"));
    for _ in 0..2 {
        for bar in riff {
            for ch in bar.chars() {
                if let Some(x) = step(ch) {
                    ev.push((t + slop(), x, ""));
                }
                t += s;
            }
        }
    }

    // Whammy: a single-note riff, every note swooping up an octave.
    ev.push((t, Local::Hold, "sirius + whammy"));
    let line = [
        130.81, 130.81, 155.56, 130.81, 174.61, 155.56, 185.0, 174.61, //
        130.81, 130.81, 116.54, 130.81, 155.56, 130.81, 98.0, 116.54,
    ];
    for _ in 0..2 {
        for f in line {
            ev.push((t + slop(), Local::Play(vec![Note::new(f).vel(0.85)], 0.0, false), ""));
            t += e;
        }
    }

    // Whammy off: staccato hits (the gate makes the gaps silent), then a
    // last chord left to ring, then muted into the room.
    ev.push((t, Local::Hold, "staccato + last chord"));
    for ch in "C.C.E...".chars() {
        if let Some(x) = step(ch) {
            ev.push((t + slop(), x, ""));
        }
        t += e;
    }
    ev.push((t, Local::Play(F5.iter().map(|f| Note::new(*f).vel(1.0).t60(8.0)).collect(), 0.006, false), ""));
    t += 8.0 * q;
    ev.push((t, Local::Play(vec![Note::new(LOW_C).vel(0.01).t60(0.02)], 0.0, false), "muted: reverb tail"));
    (ev, t + 3.0)
}

/// The score as pedalkit events.
pub fn score() -> (Score, f64) {
    let (ev, total) = local_score();
    (ev.into_iter().map(|(t, e, l)| (t as f64, e.into(), l)).collect(), total as f64)
}

impl From<Local> for Ev {
    fn from(e: Local) -> Ev {
        match e {
            Local::Play(notes, strum, ring) => Ev::Play(notes, strum, ring),
            Local::Press => Ev::Press,
            Local::Hold => Ev::Hold,
            Local::Scrape(secs) => Ev::Custom(0, secs),
        }
    }
}
