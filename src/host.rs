//! Host-only demo renderer: `cargo sim [OUT.wav]`.
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

use crate::dsp::Biquad;
use crate::guitar::{Note, Player};
use crate::*;

const N: usize = 128;
const SRU: usize = 48_000;
/// The modelled guitar into this patch, matched to the level the patch was voiced for.
const INPUT_TRIM: f32 = 0.6;

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

enum Ev {
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
fn step(ch: char) -> Option<Ev> {
    let pc = |c: &[f32]| Ev::Play(chord(c, 0.95), 0.004, false);
    Some(match ch {
        'x' => Ev::Play(vec![Note::new(LOW_C).pm().vel(0.95)], 0.0, false),
        'C' => pc(&C5),
        'E' => pc(&EB5),
        'F' => pc(&F5),
        'T' => pc(&GB5),
        // A pinch harmonic on the G string: a high partial, bent up and shaken.
        'h' => Ev::Play(vec![Note::new(784.0).vel(1.0).t60(1.5).bend(0.06, 0.12, 100.0).vib()], 0.0, false),
        '.' => Ev::Play(vec![], 0.0, false),
        _ => return None,
    })
}

fn score() -> (Vec<(f64, Ev, &'static str)>, f64) {
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
    ev.push((0.0, Ev::Press, ""));
    let prog: [&[f32]; 4] = [&CM9, &ABMAJ7, &BB, &FSUS2];
    for _ in 0..2 {
        for c in prog {
            let top = c.len() - 1;
            for (k, i) in [0, 1, 2, 3, top, 3, 2, 1].into_iter().enumerate() {
                let vel = if k == 0 { 0.7 } else { 0.55 };
                ev.push((t + slop(), Ev::Play(vec![Note::new(c[i]).vel(vel)], 0.0, k != 0), ""));
                t += e;
            }
        }
    }

    // Sirius: a pick scrape leads in, then the chug riff, twice round.
    ev.push((t, Ev::Press, "sirius: pick scrape"));
    ev.push((t, Ev::Play(vec![], 0.0, false), ""));
    ev.push((t + 0.02, Ev::Scrape((2.0 * q - 0.06) as f32), ""));
    t += 2.0 * q;
    let riff = ["x.xxx.xxC---.xx.", "x.xxx.xxE---.F--", "x.xxx.xxC---.xx.", "T---F---h-------"];
    ev.push((t, Ev::Play(vec![], 0.0, true), "sirius (riff)"));
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
    ev.push((t, Ev::Hold, "sirius + whammy"));
    let line = [
        130.81, 130.81, 155.56, 130.81, 174.61, 155.56, 185.0, 174.61, //
        130.81, 130.81, 116.54, 130.81, 155.56, 130.81, 98.0, 116.54,
    ];
    for _ in 0..2 {
        for f in line {
            ev.push((t + slop(), Ev::Play(vec![Note::new(f).vel(0.85)], 0.0, false), ""));
            t += e;
        }
    }

    // Whammy off: staccato hits (the gate makes the gaps silent), then a
    // last chord left to ring, then muted into the room.
    ev.push((t, Ev::Hold, "staccato + last chord"));
    for ch in "C.C.E...".chars() {
        if let Some(x) = step(ch) {
            ev.push((t + slop(), x, ""));
        }
        t += e;
    }
    ev.push((t, Ev::Play(F5.iter().map(|f| Note::new(*f).vel(1.0).t60(8.0)).collect(), 0.006, false), ""));
    t += 8.0 * q;
    ev.push((t, Ev::Play(vec![Note::new(LOW_C).vel(0.01).t60(0.02)], 0.0, false), "muted: reverb tail"));
    (ev, t + 3.0)
}

/// A pick dragged along the wound strings: the pick edge clicks over each
/// winding (a pulse train whose rate falls as it slows), heard through the
/// strings' bright, hissy resonance, which drops as it moves up the neck.
struct Scrape {
    bp: Biquad,
    i: usize,
    len: usize,
    ph: f32,
    seed: u32,
}

impl Scrape {
    const LEVEL: f32 = 0.35;

    fn new() -> Scrape {
        Scrape { bp: Biquad::NEW, i: 0, len: 0, ph: 0.0, seed: 0x0bad_5eed }
    }

    fn start(&mut self, secs: f32) {
        self.i = 0;
        self.len = (secs * SRU as f32) as usize;
    }

    fn tick(&mut self) -> f32 {
        if self.i >= self.len {
            return 0.0;
        }
        let p = self.i as f32 / self.len as f32;
        if self.i % 32 == 0 {
            self.bp.bandpass(3200.0 * (300.0f32 / 3200.0).powf(p), 2.5);
        }
        self.ph += 1600.0 * (180.0f32 / 1600.0).powf(p) / SRU as f32;
        let click = if self.ph >= 1.0 {
            self.ph -= 1.0;
            1.0
        } else {
            0.0
        };
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        let noise = self.seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
        let t = self.i as f32 / SRU as f32;
        let left = (self.len - self.i) as f32 / SRU as f32;
        let env = (t / 0.03).min(1.0) * (left / 0.12).min(1.0);
        self.i += 1;
        self.bp.process(click + 0.25 * noise) * env * Self::LEVEL
    }
}

pub fn main() {
    let out = std::env::args().nth(1).unwrap_or("target/render/gojira_demo.wav".into());
    let env = PatchEnv { abi_version: PATCH_ABI_VERSION as u32, user_ctx: core::ptr::null_mut() };
    let mut work = vec![0.0f32; 16];
    unsafe {
        patch_init(&env);
        set_buffer(&env, work.as_mut_ptr());
        for i in 0..PARAM_COUNT {
            set_param(&env, i, get_param_default(&env, i));
        }
    }

    let (mut events, total_s) = score();
    events.sort_by(|a, b| b.0.total_cmp(&a.0));
    let total = (total_s * SRU as f64) as usize;
    let mut guitar = Player::new();
    let mut scrape = Scrape::new();

    let mut lo = vec![];
    let mut ro = vec![];
    let mut sections: Vec<(usize, &str)> = vec![(0, "magma (clean + space)")];
    let mut t = 0;
    while t < total {
        while events.last().is_some_and(|(et, _, _)| (*et * SRU as f64) as usize <= t) {
            let (_, e, label) = events.pop().unwrap();
            match e {
                Ev::Press => unsafe { special_action(&env, ACTION_FOOTSWITCH_PRESS) },
                Ev::Hold => unsafe { special_action(&env, ACTION_FOOTSWITCH_HOLD) },
                Ev::Play(notes, strum, ring) => guitar.play(&notes, strum, ring),
                Ev::Scrape(secs) => scrape.start(secs),
            }
            if !label.is_empty() {
                sections.push((t, label));
            }
        }
        let mut l = [0.0f32; N];
        let mut r = [0.0f32; N];
        for x in l.iter_mut() {
            *x = (guitar.tick() + scrape.tick()) * INPUT_TRIM;
        }
        unsafe { update_buffers(&env, l.as_mut_ptr(), r.as_mut_ptr(), N as i32) };
        lo.extend_from_slice(&l);
        ro.extend_from_slice(&r);
        t += N;
    }

    let mut bad = false;
    sections.push((lo.len(), ""));
    println!("{:<30} {:>7} {:>8} {:>8} {:>8}", "section", "peak", "rms dB", "L-R dB", "L/R corr");
    for w in sections.windows(2) {
        let (a, name) = w[0];
        let b = w[1].0;
        let (l, r) = (&lo[a..b], &ro[a..b]);
        let peak = l.iter().chain(r).fold(0.0f32, |m, v| m.max(v.abs()));
        let rms = |x: &[f32]| (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
        let side: Vec<f32> = l.iter().zip(r).map(|(a, b)| a - b).collect();
        let dot: f64 = l.iter().zip(r).map(|(a, b)| *a as f64 * *b as f64).sum();
        let corr = dot / (rms(l) * rms(r) * l.len() as f64).max(1e-12);
        let to_db = |v: f64| 20.0 * v.max(1e-9).log10();
        println!("{name:<30} {peak:>7.3} {:>8.1} {:>8.1} {corr:>8.2}", to_db(rms(l)), to_db(rms(&side)));
        if l.iter().chain(r).any(|v| !v.is_finite()) || peak >= 1.0 {
            bad = true;
        }
    }
    println!("final LED colour index: {}", unsafe { get_state_idx(&env) });

    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    write_wav(&out, &lo, &ro);
    println!("wrote {out} ({:.1} s)", lo.len() as f64 / SRU as f64);
    if bad {
        eprintln!("error: NaN or full-scale output");
        std::process::exit(1);
    }
    let _ = (get_buffer_size, get_param_min, get_param_max, is_param_enabled, get_param_name, get_param_unit);
}

fn write_wav(path: &str, l: &[f32], r: &[f32]) {
    let bytes = (l.len() * 8) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + bytes).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&(48_000u32 * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&bytes.to_le_bytes());
    for (a, c) in l.iter().zip(r) {
        b.extend_from_slice(&a.to_le_bytes());
        b.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}
