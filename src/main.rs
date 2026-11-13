//! gojira: a Polyend Endless patch for the most Gojira guitar tone possible:
//! a tight, crushing, down-tuned modern high-gain rhythm tone and a lush,
//! ambient clean.
//!
//! Two voices run all the time and the footswitch crossfades between them:
//!   Magma   clean amp with gentle compression, a stereo chorus, a dotted-
//!           eighth echo and a big room (the ambient Gojira cleans)
//!   Sirius  a tight overdrive boost (it clips only the mids, so the low end
//!           stays tight) into a two-stage high-gain preamp, a deep, slightly
//!           scooped tone stack and a power-amp squash, behind a fast noise
//!           gate for the staccato stops
//! then a closed-back 4x12 cab sim and a stereo reverb shared by both.
//! Holding the footswitch arms a Whammy (octave up, before the amp): with
//! Sirius on, every picked note swoops up an octave, like rocking a Whammy
//! pedal toe-down in time with the riff.
//!
//! Controls
//!   Left knob   Gain    (preamp gain, ~22 to 46 dB into a fixed second stage)
//!   Mid knob    Space   (chorus, echo and reverb; CCW = dry)
//!   Right knob  Level   (output; unity at 3 o'clock)
//!   Footswitch press  Magma <-> Sirius (the clean sits ~5 dB under the distortion)
//!   Footswitch hold   Whammy on/off (sounds in Sirius only)
//!   LED  Magma: light blue | Sirius: blue | with Whammy armed: dim cyan | magenta
//!
//! Host builds (`cargo sim`) render a demo riff through the same code; see `host`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod abi;
#[allow(dead_code)]
mod dsp;

use core::cell::UnsafeCell;
use core::f32::consts::TAU;

use abi::*;
use dsp::*;

// ---------------------------------------------------------------- voicing

/// Preamp stage 1 gain (dB) at knob 0, and the extra at knob 1.
const GAIN_MIN_DB: f32 = 22.0;
const GAIN_RANGE_DB: f32 = 24.0;
/// Preamp stage 2 gain, dB.
const GAIN2_DB: f32 = 14.0;
/// Stage biases: asymmetric clipping (even harmonics, like a tube stage).
const BIAS1: f32 = 0.3;
const BIAS2: f32 = -0.2;
/// Tight boost: only the content above BOOST_HP is driven into the boost's
/// clipper, so the low end reaches the amp clean and tight.
const BOOST_HP: f32 = 720.0;
const BOOST_DRIVE: f32 = 4.0;
const BOOST_CLIP: f32 = 0.25;
/// Power-amp squash.
const POWER_DRIVE: f32 = 1.3;

/// The clean sits this far under the distortion (dB), plus the clean amp's
/// makeup gain so it lands there.
const MAGMA_LEVEL_DB: f32 = -5.0;
const CLEAN_DRIVE: f32 = 1.4;
const CLEAN_MAKEUP_DB: f32 = 22.0;

/// Chorus (clean only): centre delay and depth (samples, 7 ms +- 2 ms), rate
/// (Hz) and wet level at full Space.
const CHO_DELAY: f32 = 336.0;
const CHO_DEPTH: f32 = 96.0;
const CHO_RATE: f32 = 0.7;
const CHO_MIX: f32 = 0.6;
/// Echo (clean only): 375 ms (a dotted eighth at 120 bpm), feedback, the
/// low-pass in its loop (Hz) and send at full Space. The send is cut on a
/// switch to Sirius, not the return, so the repeats ring on across it.
const ECHO_TIME: f32 = 18_000.0;
const ECHO_FB: f32 = 0.38;
const ECHO_LP: f32 = 3000.0;
const ECHO_SEND: f32 = 0.55;

/// Reverb: wet level at full Space for each voice (the clean is drenched,
/// the distortion only gets a little air), room feedback range, damping and
/// pre-delay (samples, 22 ms).
const MAGMA_WET: f32 = 1.0;
const SIRIUS_WET: f32 = 0.12;
const ROOM_MIN: f32 = 0.78;
const ROOM_RANGE: f32 = 0.17;
const DAMP: f32 = 0.4;
const PREDELAY: f32 = 1056.0;

/// Whammy: pitch-shifter window (samples, ~37 ms: long enough for drop-C
/// lows), interval (semitones), the swoop's time constant (s), the onset
/// detector (fast envelope this many times the slow one, then locked out
/// this long, s) and the wet crossfade time when armed (s).
const WH_WIN: f32 = 1800.0;
const WH_SEMIS: f32 = 12.0;
const WH_RISE_S: f32 = 0.045;
const WH_ONSET: f32 = 1.8;
const WH_LOCK_S: f32 = 0.07;
const WH_FADE_S: f32 = 0.02;

/// Noise gate on the distortion (input envelope, linear, with hysteresis):
/// fast both ways, so the staccato stops are dead silent.
const GATE_OPEN: f32 = 0.0012;
const GATE_CLOSE: f32 = 0.0005;

/// Output trim: unity Level puts the distorted riff around -14 dB RMS.
const OUT_TRIM: f32 = 0.32;

// ---------------------------------------------------------------- reverb

/// Freeverb-style lowpass-feedback comb with a runtime length.
struct Comb<const N: usize> {
    buf: [f32; N],
    len: usize,
    i: usize,
    filt: f32,
}

impl<const N: usize> Comb<N> {
    const NEW: Self = Self { buf: [0.0; N], len: N, i: 0, filt: 0.0 };

    #[inline]
    fn process(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let o = self.buf[self.i];
        self.filt = o + (self.filt - o) * damp;
        self.buf[self.i] = x + self.filt * fb;
        self.i += 1;
        if self.i >= self.len {
            self.i = 0;
        }
        o
    }
}

/// Schroeder allpass (feedback 0.5) with a runtime length.
struct Allpass<const N: usize> {
    buf: [f32; N],
    len: usize,
    i: usize,
}

impl<const N: usize> Allpass<N> {
    const NEW: Self = Self { buf: [0.0; N], len: N, i: 0 };

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let o = self.buf[self.i];
        self.buf[self.i] = x + o * 0.5;
        self.i += 1;
        if self.i >= self.len {
            self.i = 0;
        }
        o - x
    }
}

/// Freeverb tunings (44.1 kHz) scaled to 48 kHz; the right channel is spread.
const COMB_LEN: [usize; 8] = [1214, 1293, 1389, 1475, 1547, 1622, 1694, 1759];
const AP_LEN: [usize; 4] = [605, 480, 371, 245];
const SPREAD: usize = 25;

struct Reverb {
    pre: Delay<2048>,
    cl: [Comb<2048>; 8],
    cr: [Comb<2048>; 8],
    al: [Allpass<1024>; 4],
    ar: [Allpass<1024>; 4],
}

impl Reverb {
    const NEW: Self = Self {
        pre: Delay::NEW,
        cl: [Comb::NEW; 8],
        cr: [Comb::NEW; 8],
        al: [Allpass::NEW; 4],
        ar: [Allpass::NEW; 4],
    };

    fn init(&mut self) {
        for k in 0..8 {
            self.cl[k].len = COMB_LEN[k];
            self.cr[k].len = COMB_LEN[k] + SPREAD;
        }
        for k in 0..4 {
            self.al[k].len = AP_LEN[k];
            self.ar[k].len = AP_LEN[k] + SPREAD;
        }
    }

    /// Wet only, stereo, at roughly the input's level.
    #[inline]
    fn process(&mut self, x: f32, room: f32) -> (f32, f32) {
        self.pre.push(x);
        let x = self.pre.read(PREDELAY) * 0.015;
        let mut l = 0.0;
        let mut r = 0.0;
        for c in self.cl.iter_mut() {
            l += c.process(x, room, DAMP);
        }
        for c in self.cr.iter_mut() {
            r += c.process(x, room, DAMP);
        }
        for a in self.al.iter_mut() {
            l = a.process(l);
        }
        for a in self.ar.iter_mut() {
            r = a.process(r);
        }
        (l * 3.0, r * 3.0)
    }
}

// ---------------------------------------------------------------- state

struct Gj {
    ready: bool,
    error: bool,
    // knobs, 0..1
    gain: f32,
    space: f32,
    level: f32,
    loud: bool,
    whammy: bool,
    // smoothed values and targets
    g_pre: f32,
    g_pre_t: f32,
    out_g: f32,
    out_t: f32,
    space_s: f32,
    mix: f32,
    // filters
    hp_in: Biquad,
    boost_hp: Biquad,
    boost_lp: Biquad,
    pre_hp: Biquad,
    inter_hp: Biquad,
    inter_lp: Biquad,
    tone_low: Biquad,
    tone_mid: Biquad,
    tone_pres: Biquad,
    tone_dc: Biquad,
    tone_lp: Biquad,
    clean_hp: Biquad,
    clean_low: Biquad,
    clean_sparkle: Biquad,
    cab: [Biquad; 6],
    u1: f32,
    u2: f32,
    // dynamics
    env: f32,
    env_slow: f32,
    gate: f32,
    gate_open: bool,
    comp: f32,
    // whammy
    wh_line: Delay<4096>,
    wh_ph: f32,
    wh_semi: f32,
    wh_mix: f32,
    wh_lock: u32,
    // chorus and echo
    cho_line: Delay<1024>,
    cho_ph: f32,
    echo: Delay<32768>,
    echo_lp: OnePole,
    // reverb
    rev: Reverb,
    // coefficients
    c_param: f32,
    c_mix: f32,
    c_att: f32,
    c_rel: f32,
    c_slow: f32,
    c_gate_open: f32,
    c_gate_close: f32,
    c_comp: f32,
    c_wh_rise: f32,
    c_wh_fade: f32,
}

impl Gj {
    const NEW: Self = Self {
        ready: false,
        error: false,
        gain: 0.7,
        space: 0.5,
        level: 0.75,
        loud: true,
        whammy: false,
        g_pre: 1.0,
        g_pre_t: 1.0,
        out_g: 0.0,
        out_t: 0.0,
        space_s: 0.5,
        mix: 1.0,
        hp_in: Biquad::NEW,
        boost_hp: Biquad::NEW,
        boost_lp: Biquad::NEW,
        pre_hp: Biquad::NEW,
        inter_hp: Biquad::NEW,
        inter_lp: Biquad::NEW,
        tone_low: Biquad::NEW,
        tone_mid: Biquad::NEW,
        tone_pres: Biquad::NEW,
        tone_dc: Biquad::NEW,
        tone_lp: Biquad::NEW,
        clean_hp: Biquad::NEW,
        clean_low: Biquad::NEW,
        clean_sparkle: Biquad::NEW,
        cab: [Biquad::NEW; 6],
        u1: 0.0,
        u2: 0.0,
        env: 0.0,
        env_slow: 0.0,
        gate: 0.0,
        gate_open: false,
        comp: 1.0,
        wh_line: Delay::NEW,
        wh_ph: 0.0,
        wh_semi: WH_SEMIS,
        wh_mix: 0.0,
        wh_lock: 0,
        cho_line: Delay::NEW,
        cho_ph: 0.0,
        echo: Delay::NEW,
        echo_lp: OnePole::NEW,
        rev: Reverb::NEW,
        c_param: 1.0,
        c_mix: 1.0,
        c_att: 1.0,
        c_rel: 1.0,
        c_slow: 1.0,
        c_gate_open: 1.0,
        c_gate_close: 1.0,
        c_comp: 1.0,
        c_wh_rise: 1.0,
        c_wh_fade: 1.0,
    };

    fn init(&mut self) {
        self.hp_in.highpass(80.0, 0.707);
        // Sirius: a tight boost (mid hump, lows left alone) ...
        self.boost_hp.highpass(BOOST_HP, 0.5);
        self.boost_lp.lowpass(5000.0, 0.707);
        // ... into a modern high-gain preamp, tightened again at its input.
        self.pre_hp.highpass(100.0, 0.707);
        self.inter_hp.highpass(140.0, 0.707); // also strips stage 1's DC
        self.inter_lp.lowpass(6000.0, 0.707);
        // Deep low end, mids pulled back a little, crisp presence.
        self.tone_low.peak(90.0, 0.9, 4.0);
        self.tone_mid.peak(480.0, 0.9, -4.5);
        self.tone_pres.peak(3200.0, 0.9, 4.0);
        self.tone_dc.highpass(30.0, 0.707); // stage 2's DC
        self.tone_lp.lowpass(7500.0, 0.707);
        // Magma: a clean channel with a little sparkle.
        self.clean_hp.highpass(90.0, 0.707);
        self.clean_low.peak(250.0, 0.8, -2.0);
        self.clean_sparkle.peak(2800.0, 0.8, 3.0);
        // Closed-back 4x12 with V30-style speakers: low thump, upper-mid
        // bark, a steep roll-off above ~5 kHz.
        self.cab[0].highpass(75.0, 0.8);
        self.cab[1].peak(110.0, 1.2, 3.0);
        self.cab[2].peak(400.0, 1.0, -2.0);
        self.cab[3].peak(2300.0, 1.2, 3.0);
        self.cab[4].lowpass(5500.0, 0.8);
        self.cab[5].lowpass(6500.0, 0.6);
        self.echo_lp.set(ECHO_LP);
        self.rev.init();
        self.c_param = coef(0.02);
        self.c_mix = coef(0.012);
        self.c_att = coef(0.0005);
        self.c_rel = coef(0.03);
        self.c_slow = coef(0.03);
        self.c_gate_open = coef(0.0005);
        self.c_gate_close = coef(0.03);
        self.c_comp = coef(0.05);
        self.c_wh_rise = coef(WH_RISE_S);
        self.c_wh_fade = coef(WH_FADE_S);
        self.retarget();
        self.g_pre = self.g_pre_t;
        self.out_g = self.out_t;
        self.space_s = self.space;
        self.mix = if self.loud { 1.0 } else { 0.0 };
        self.ready = true;
    }

    fn retarget(&mut self) {
        self.g_pre_t = db(GAIN_MIN_DB + GAIN_RANGE_DB * self.gain);
        // Square law: 0 at minimum, unity at 3 o'clock, +5 dB at maximum.
        let v = self.level / 0.75;
        self.out_t = v * v * OUT_TRIM;
    }

    /// Whammy: two crossfaded taps sweeping through a short delay line (the
    /// read point moves at `ratio` times the write point).
    #[inline]
    fn shift(&mut self, x: f32, ratio: f32) -> f32 {
        self.wh_line.push(x);
        self.wh_ph += (ratio - 1.0) * (1.0 / WH_WIN);
        self.wh_ph -= floor(self.wh_ph);
        let mut p2 = self.wh_ph + 0.5;
        if p2 >= 1.0 {
            p2 -= 1.0;
        }
        // Hann windows half a window apart sum to 1; each is silent when its
        // tap jumps back.
        let g1 = 0.5 - 0.5 * cos(TAU * self.wh_ph);
        let a = self.wh_line.read((1.0 - self.wh_ph) * WH_WIN + 1.0);
        let b = self.wh_line.read((1.0 - p2) * WH_WIN + 1.0);
        a * g1 + b * (1.0 - g1)
    }

    /// Sirius voice: tight boost -> two-stage high-gain preamp -> tone stack
    /// -> power-amp squash (mono).
    #[inline]
    fn dist(&mut self, x: f32) -> f32 {
        // Tight boost, used the way metal players use a Tube Screamer: low
        // drive, and only the mids and up are clipped, so the low end stays
        // tight and the amp gets a mid hump to bite on.
        let x = x + tanh(self.boost_hp.process(x) * BOOST_DRIVE) * BOOST_CLIP;
        let x = self.pre_hp.process(self.boost_lp.process(x));
        // Stage 1: asymmetric saturation, cheap 2x oversampling.
        let u = x * self.g_pre;
        let y = 0.5 * (stage(0.5 * (u + self.u1), BIAS1) + stage(u, BIAS1));
        self.u1 = u;
        let y = self.inter_lp.process(self.inter_hp.process(y));
        // Stage 2.
        let u = y * db(GAIN2_DB);
        let y = 0.5 * (stage(0.5 * (u + self.u2), BIAS2) + stage(u, BIAS2));
        self.u2 = u;
        let y = self.tone_pres.process(self.tone_mid.process(self.tone_low.process(y)));
        let y = self.tone_lp.process(self.tone_dc.process(y));
        tanh(y * POWER_DRIVE) * (1.0 / POWER_DRIVE)
    }

    /// Magma voice: clean amp with gentle compression (mono).
    #[inline]
    fn clean(&mut self, x: f32) -> f32 {
        let x = self.clean_hp.process(x);
        let a = x.abs() * CLEAN_DRIVE;
        let target = if a > 0.5 { 0.5 / a } else { 1.0 };
        self.comp += (target - self.comp) * if target < self.comp { 0.05 } else { self.c_comp };
        let y = tanh(x * CLEAN_DRIVE * self.comp) * 0.7;
        self.clean_sparkle.process(self.clean_low.process(y))
    }

    #[inline]
    fn tick(&mut self, x: f32) -> (f32, f32) {
        self.g_pre += (self.g_pre_t - self.g_pre) * self.c_param;
        self.out_g += (self.out_t - self.out_g) * self.c_param;
        self.space_s += (self.space - self.space_s) * self.c_param;
        let mix_t = if self.loud { 1.0 } else { 0.0 };
        self.mix += (mix_t - self.mix) * self.c_mix;

        let x = self.hp_in.process(x);
        let a = x.abs();
        self.env += (a - self.env) * if a > self.env { self.c_att } else { self.c_rel };
        self.env_slow += (self.env - self.env_slow) * self.c_slow;
        if self.env > GATE_OPEN {
            self.gate_open = true;
        } else if self.env < GATE_CLOSE {
            self.gate_open = false;
        }
        let (gt, gc) = if self.gate_open { (1.0, self.c_gate_open) } else { (0.0, self.c_gate_close) };
        self.gate += (gt - self.gate) * gc;

        // Whammy: each pick attack drops the pedal to the heel and it rocks
        // straight back up an octave, so every note swoops up into the
        // octave and stays there until the next one.
        if self.wh_lock > 0 {
            self.wh_lock -= 1;
        } else if self.env > WH_ONSET * self.env_slow && self.env > 2.0 * GATE_OPEN {
            self.wh_semi = 0.0;
            self.wh_lock = (WH_LOCK_S * SR) as u32;
        }
        self.wh_semi += (WH_SEMIS - self.wh_semi) * self.c_wh_rise;
        let wt = if self.whammy && self.loud { 1.0 } else { 0.0 };
        self.wh_mix += (wt - self.wh_mix) * self.c_wh_fade;
        let s = self.shift(x, exp2(self.wh_semi * (1.0 / 12.0)));
        let xd = x + (s - x) * self.wh_mix;

        // Both voices always run, so switching is a click-free crossfade.
        let d = self.dist(xd) * self.gate;
        let c = self.clean(x) * db(MAGMA_LEVEL_DB + CLEAN_MAKEUP_DB);
        let m = self.mix;
        let mut y = c + (d - c) * m;
        for b in self.cab.iter_mut() {
            y = b.process(y);
        }

        // Stereo chorus on the clean: two taps swept in quadrature.
        let sp = self.space_s;
        self.cho_line.push(y);
        self.cho_ph += CHO_RATE / SR;
        if self.cho_ph >= 1.0 {
            self.cho_ph -= 1.0;
        }
        let w = TAU * self.cho_ph;
        let cho = CHO_MIX * sp * (1.0 - m);
        let yl = y + cho * self.cho_line.read(CHO_DELAY + CHO_DEPTH * sin(w));
        let yr = y + cho * self.cho_line.read(CHO_DELAY + CHO_DEPTH * cos(w));

        // Echo, then the shared reverb (so its tail carries across a switch).
        let e = self.echo.read(ECHO_TIME);
        let fb = self.echo_lp.process(e) * ECHO_FB;
        self.echo.push(0.5 * (yl + yr) * ECHO_SEND * sp * (1.0 - m) + fb);
        let room = ROOM_MIN + ROOM_RANGE * sp;
        let (wl, wr) = self.rev.process(0.5 * (yl + yr) + e, room);
        let wet = sp * (MAGMA_WET + (SIRIUS_WET - MAGMA_WET) * m);
        (limit((yl + e + wet * wl) * self.out_g), limit((yr + e + wet * wr) * self.out_g))
    }

    fn led(&self) -> Color {
        if self.error {
            return Color::Red;
        }
        match (self.loud, self.whammy) {
            (false, false) => Color::LightBlue,
            (false, true) => Color::DimCyan,
            (true, false) => Color::Blue,
            (true, true) => Color::Magenta,
        }
    }
}

/// Preamp stage: asymmetric tube-style saturation, zero at rest.
#[inline]
fn stage(u: f32, bias: f32) -> f32 {
    tanh(u + bias) - tanh(bias)
}

/// Output soft limiter, never reaching ±1.
#[inline]
fn limit(x: f32) -> f32 {
    tanh(x) * 0.98
}

/// All patch state. The firmware calls every entry point from the audio
/// thread (SDK: "no need to synchronize").
struct Global(UnsafeCell<Gj>);
unsafe impl Sync for Global {}
static STATE: Global = Global(UnsafeCell::new(Gj::NEW));

#[allow(clippy::mut_from_ref)]
fn state() -> &'static mut Gj {
    // SAFETY: single-threaded access from the firmware's audio thread.
    unsafe { &mut *STATE.0.get() }
}

// ---------------------------------------------------------------- entry points

const PARAM_GAIN: i32 = 0;
const PARAM_SPACE: i32 = 1;
const PARAM_LEVEL: i32 = 2;

unsafe extern "C" fn patch_init(env: *const PatchEnv) {
    if env.is_null() || unsafe { (*env).abi_version } != PATCH_ABI_VERSION as u32 {
        return;
    }
    state().init();
}

unsafe extern "C" fn update_buffers(_: *const PatchEnv, left: *mut f32, right: *mut f32, samples: i32) -> f32 {
    let Ok(n) = usize::try_from(samples) else { return 0.0 };
    if left.is_null() || right.is_null() || n == 0 {
        return 0.0;
    }
    // SAFETY: the firmware passes two buffers of `samples` floats.
    let (l, r) = unsafe { (core::slice::from_raw_parts_mut(left, n), core::slice::from_raw_parts_mut(right, n)) };
    let s = state();
    if !s.ready {
        return 0.0;
    }
    for (a, b) in l.iter_mut().zip(r.iter_mut()) {
        let (ol, or) = s.tick(*a);
        *a = ol;
        *b = or;
    }
    0.0
}

unsafe extern "C" fn set_buffer(_: *const PatchEnv, buffer: *mut f32) {
    // All state is static; the working buffer isn't needed.
    if buffer.is_null() {
        state().error = true;
    }
}

unsafe extern "C" fn get_buffer_size(_: *const PatchEnv) -> i32 {
    WORKING_BUFFER_SIZE
}

unsafe extern "C" fn get_param_min(_: *const PatchEnv, _: i32) -> f32 {
    0.0
}

unsafe extern "C" fn get_param_max(_: *const PatchEnv, _: i32) -> f32 {
    1.0
}

unsafe extern "C" fn get_param_default(_: *const PatchEnv, idx: i32) -> f32 {
    match idx {
        PARAM_GAIN => 0.7,
        PARAM_SPACE => 0.5,
        _ => 0.75,
    }
}

unsafe extern "C" fn is_param_enabled(_: *const PatchEnv, idx: i32, source_id: i32) -> u8 {
    // Knobs only (source 0), like the SDK wrapper.
    (source_id == 0 && (0..PARAM_COUNT).contains(&idx)) as u8
}

unsafe fn write_c_str(s: &str, out: *mut u8, len: usize) {
    if out.is_null() || len == 0 {
        return;
    }
    let n = s.len().min(len - 1);
    // SAFETY: the firmware provides `len` writable bytes at `out`.
    unsafe {
        core::ptr::copy_nonoverlapping(s.as_ptr(), out, n);
        *out.add(n) = 0;
    }
}

unsafe extern "C" fn get_param_name(_: *const PatchEnv, idx: i32, out: *mut u8, len: usize) {
    let name = match idx {
        PARAM_GAIN => "Gain",
        PARAM_SPACE => "Space",
        PARAM_LEVEL => "Level",
        _ => "",
    };
    unsafe { write_c_str(name, out, len) }
}

unsafe extern "C" fn get_param_unit(_: *const PatchEnv, _: i32, out: *mut u8, len: usize) {
    unsafe { write_c_str("", out, len) }
}

unsafe extern "C" fn set_param(_: *const PatchEnv, idx: i32, value: f32) {
    let s = state();
    let v = value.clamp(0.0, 1.0);
    match idx {
        PARAM_GAIN => s.gain = v,
        PARAM_SPACE => s.space = v,
        PARAM_LEVEL => s.level = v,
        _ => return,
    }
    if s.ready {
        s.retarget();
    }
}

unsafe extern "C" fn special_action(_: *const PatchEnv, idx: i32) {
    let s = state();
    match idx {
        ACTION_FOOTSWITCH_PRESS => s.loud = !s.loud,
        ACTION_FOOTSWITCH_HOLD => s.whammy = !s.whammy,
        _ => return,
    }
    if s.ready {
        s.retarget();
    }
}

unsafe extern "C" fn get_state_idx(_: *const PatchEnv) -> i32 {
    state().led() as i32
}

// ---------------------------------------------------------------- header

#[cfg(target_os = "none")]
unsafe extern "C" {
    static __patch_bss_start: u8;
    static patch_image_size: u8;
    static patch_bss_size: u8;
}

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".patch_header")]
#[used]
pub static patch_header: PatchHeader = PatchHeader {
    magic: PATCH_MAGIC,
    abi_version: PATCH_ABI_VERSION,
    flags: PATCH_FLAG_NONE,
    init: patch_init,
    agent_update_buffers: update_buffers,
    agent_set_buffer: set_buffer,
    agent_get_buffer_size: get_buffer_size,
    agent_get_param_min: get_param_min,
    agent_get_param_max: get_param_max,
    agent_get_param_default: get_param_default,
    agent_is_param_enabled: is_param_enabled,
    agent_get_param_name: get_param_name,
    agent_get_param_unit: get_param_unit,
    agent_set_param: set_param,
    agent_special_action: special_action,
    agent_get_state_idx: get_state_idx,
    image_size: &raw const patch_image_size,
    bss_begin: &raw const __patch_bss_start,
    bss_size: &raw const patch_bss_size,
    reserved: [0; 16],
};

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(not(target_os = "none"))]
#[allow(dead_code)] // each demo uses only some of the playing techniques
mod guitar;
#[cfg(not(target_os = "none"))]
mod host;

#[cfg(not(target_os = "none"))]
fn main() {
    host::main();
}
