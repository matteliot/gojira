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
//! The sound lives in `gojira` (built from `pedalkit` blocks set by its
//! `presets`); the entry points come from `pedalkit::endless_patch!`.
//! Host builds (`cargo sim`) render a demo riff through the same code; see `host`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

/// Reusable blocks (effects, DSP, the Endless ABI); see its docs.
mod pedalkit;

/// The pedal itself.
mod gojira;

endless_patch!(kit = crate::pedalkit, pedal = crate::gojira::Patch);

#[cfg(not(target_os = "none"))]
mod host;

#[cfg(not(target_os = "none"))]
fn main() {
    host::main();
}
