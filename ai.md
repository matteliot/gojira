# ai.md — Project Requirements Log

This file records every requirement given for the **gojira** project, so the whole project can be recreated from scratch by handing this file to an AI assistant as the prompt.

It is kept up to date as new requirements are given. Requirements are listed in the order they were given; later entries override earlier ones where they conflict (and the earlier entry is marked as superseded).

---

## Meta / Process Requirements

1. Maintain this `ai.md` file at the repo root. Its purpose is to keep a list of all requirements so the entire project can be recreated from this file alone. Update it as work progresses, whenever a new requirement is given. Keep lines unwrapped (wrap only around 200 characters at a natural break).

## Project Requirements

2. **In the existing `gojira` directory, do the same thing as `../jesusMaryChain` (github.com/matteliot/jesusMaryChain; read its `ai.md`), but for the French metal band Gojira:** a Polyend Endless patch for the most Gojira guitar tone possible, with one regular tone and one distorted tone, built with `cargo build --release`.

### Implementation decisions (needed to recreate requirement 2)

- **Toolchain and packaging (copied unchanged from `../jesusMaryChain`).** `no_std` Rust, edition 2024, nightly, `thumbv7em-none-eabihf`. Copied: `src/abi.rs`, `src/dsp.rs`, `src/guitar.rs` (host-only guitar model), `patch.ld`, `build.rs`, `.cargo/config.toml` (`-fp64`, `cargo sim` alias), `rust-toolchain.toml`, `.gitignore`. `build-endl.sh` makes `target/endless/gojira.endl`, checks for f64 instructions and the 512 KB limit; `--deploy` copies to `/Volumes/ENDLESS`. The reverb (`Comb`, `Allpass`, `Reverb`), the entry points and the header are copied from jesusMaryChain's `main.rs`, with the parameters renamed.
- **Input:** left jack, HP 80 Hz. Envelope (0.5 ms attack, 30 ms release) drives a fast gate on the distorted voice: opens at 0.0012, closes at 0.0005, 0.5 ms open, 30 ms close (silent staccato stops). A slow envelope (30 ms one-pole of the fast one) is used for Whammy onset detection.
- **Magma (regular voice):** HP 90 Hz → peak compressor (as jesusMaryChain's Honey) → `tanh(1.4x)·0.7` → peak 250 Hz Q0.8 −2 dB → peak 2.8 kHz Q0.8 +3 dB. Gain −5 dB + 22 dB makeup (lands ~5 dB RMS under Sirius).
- **Sirius (distorted voice):**
  1. Tight boost: `x + 0.25·tanh(4·HP720(x))` (HP Q0.5: only mids and up are clipped), LP 5 kHz, then HP 100 Hz.
  2. Stage 1 gain 22–46 dB (Gain knob): `tanh(u + 0.3) − tanh(0.3)`, 2× oversampled (midpoint average). HP 140 Hz, LP 6 kHz.
  3. Stage 2: +14 dB into `tanh(u − 0.2) − tanh(−0.2)`, 2× oversampled.
  4. Tone stack: peak 90 Hz Q0.9 +4 dB, peak 480 Hz Q0.9 −4.5 dB, peak 3.2 kHz Q0.9 +4 dB, HP 30 Hz, LP 7.5 kHz. Power amp `tanh(1.3y)/1.3`. Multiplied by the gate.
- **Whammy (footswitch hold arms it; only active in Sirius):** before the amp. Pitch shifter: two taps on a 4096-sample delay line, window 1800 samples, phase advancing `(ratio − 1)/1800` per sample, tap delays `(1 − φ)·1800 + 1` and the same half a window later, Hann crossfade. On each pick onset (fast envelope > 1.8× slow envelope and > 0.0024, then locked out for 70 ms) the shift resets to 0 semitones and glides to +12 with τ = 45 ms. The wet blend fades in/out over 20 ms.
- **Mix and output:** 12 ms crossfade between voices (mono) → closed-back 4x12 cab: HP 75 Q0.8, peak 110 Hz Q1.2 +3 dB, peak 400 Hz Q1 −2 dB, peak 2.3 kHz Q1.2 +3 dB, LP 5.5 kHz Q0.8, LP 6.5 kHz Q0.6. Stereo chorus (Magma only): taps at 336 ± 96 samples, 0.7 Hz, L sine / R cosine, wet `0.6 · Space · (1 − mix)`. Echo: 18000 samples (375 ms), feedback 0.38 through a 3 kHz one-pole LP, send `0.55 · Space · (1 − mix)` (the send is cut on a switch, not the return). The echo goes to both outputs and into the reverb. Then the shared stereo reverb (as jesusMaryChain: 22 ms pre-delay, Freeverb 8 combs at 1214…1759, 4 allpasses at 605/480/371/245, right +25, input ×0.015, output ×3), damping 0.4, room `0.78 + 0.17·Space`, wet `Space × (1.0 Magma … 0.12 Sirius)` following the crossfade. Output `(dry + echo + wet·rev) × Level² (unity at 0.75) × 0.32`, then `tanh × 0.98`.
- **Controls:** Gain (default 0.7), Space (default 0.5), Level (default 0.75). Press: Magma ↔ Sirius (starts in Sirius). Hold: Whammy armed on/off. LED: Magma light blue / dim cyan, Sirius blue / magenta (Whammy off / armed). Red means the working buffer is missing.
- **Host audition:** `cargo sim` renders original drop-C material at 120 bpm, input ×0.6, ±4 ms timing slop: Magma (eighth-note arpeggios over Cm9, Abmaj7, Bb, Fsus2, a bar each, 2 rounds) → press → a 1 s pick scrape (host-side: a click train falling from 1600 to 180 Hz plus noise, band-passed at Q2.5 with a centre falling from 3.2 kHz to 300 Hz) → Sirius riff in 16ths, 2 rounds of `x.xxx.xxC---.xx.` / `x.xxx.xxE---.F--` / `x.xxx.xxC---.xx.` / `T---F---h-------` (x palm-muted low C, C/E/F/T = C5/Eb5/F5/Gb5, h = pinch-harmonic squeal at 784 Hz bent +100 cents with vibrato, `.` stop, `-` ring) → hold: Whammy on a 16-note eighth-note single-note line, 2 rounds → hold off: `C.C.E...` in eighths, F5 rung for 2 bars → mute → 3 s tail. Output `target/render/gojira_demo.wav`, with per-section peak/RMS/side/correlation stats; it fails on NaN or full-scale output.

3. **Buffers are zero-initialized in `.bss` and explicitly zeroed in `patch_init` before use** (rather than shipped as zeros inside the patch image). The delay lines (and reverb, where there is one) move out of the patch state into a separate `Bufs` static that is all zero at rest, so it lands in `.bss` instead of being stored as zeros in the `.endl` (runtime-length reverb combs/allpasses start with length 0 and get their lengths in init). The `Global` cell is generic; `tick` and the methods that use a delay line take `&mut Bufs`. `patch_init` then clears each buffer static in place with `core::ptr::write_bytes` (no heap or `calloc` in `no_std`, and a zeroed copy would be a stack temporary) before `init`: the loader should already have cleared `.bss`, but nothing promises it, and a NaN left in a feedback loop never decays. The firmware working buffer stays unused.

4. **Ship the built patch in a `release/` directory at the repo root** (`release/gojira.endl`, a copy of `target/endless/gojira.endl` from `./build-endl.sh`), committed to git, so a user can download it from GitHub and copy it straight to the pedal without building. README has an Install section saying so. Rebuild and recopy it whenever the patch changes.
