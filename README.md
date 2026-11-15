# gojira

A [Polyend Endless](https://polyend.com/endless/) patch for the most Gojira guitar tone possible: a tight, crushing,
down-tuned high-gain rhythm tone with dead-silent stops, plus a lush ambient clean. It's written in `no_std` Rust
against the patch ABI of Polyend's [FxPatchSDK](https://github.com/polyend/FxPatchSDK) (ABI 0x000B), so no ARM GCC is
needed.

Two voices run all the time and the footswitch crossfades between them:

- **Magma** (regular): a clean amp with gentle compression and a little sparkle, then a stereo chorus, a dotted-eighth
  echo (375 ms) and a big room. These are the ambient, floating Gojira cleans.
- **Sirius** (distorted): a tight boost into a modern high-gain amp. The boost is used the way metal players use a Tube
  Screamer: it clips only the mids and up, so the low end reaches the amp clean and tight. Then come two asymmetric
  preamp stages, a deep, slightly scooped tone stack with crisp presence, and a power-amp squash. A fast noise gate
  makes the staccato stops silent.

Both go through a closed-back 4x12 cab sim and a stereo Freeverb-style reverb (22 ms pre-delay). The reverb is shared,
so its tail carries across a switch, and so do the echo's repeats. Sirius gets only a little of the room so the riffs
stay tight. Input comes from the left jack.

**Whammy**: hold the footswitch to arm it. In Sirius, every picked note swoops up an octave and stays there until the
next note, as if you rocked a Whammy pedal toe-down in time with the riff. It sits before the amp, as on a real
pedalboard.

| Control | Function |
|---|---|
| Left knob | **Gain**: preamp gain, about 22 to 46 dB into a fixed second stage |
| Mid knob | **Space**: chorus, echo and reverb. CCW is dry |
| Right knob | **Level**: output, unity at 3 o'clock |
| Footswitch press | **Magma ↔ Sirius**. Magma sits about 5 dB under Sirius |
| Footswitch hold | **Whammy** armed/off. It only sounds in Sirius |

| LED | Whammy off | Whammy armed |
|---|---|---|
| Magma | light blue | dim cyan |
| Sirius | blue | magenta |

Red means the firmware didn't provide the working buffer.

## Install

Download [`release/gojira.endl`](release/gojira.endl) (on GitHub: open it and click the download button), connect the Endless
to your computer over USB and copy the file onto the pedal. Nothing needs to be built. To rebuild it from source, see below.

## Build

```sh
rustup target add thumbv7em-none-eabihf --toolchain nightly
rustup component add llvm-tools --toolchain nightly
cargo build --release        # ELF for the pedal (default target is thumbv7em-none-eabihf)
./build-endl.sh              # build + make target/endless/gojira.endl, check f64 use and size
./build-endl.sh --deploy     # ...and copy to /Volumes/ENDLESS
```

## Audition on your computer

```sh
cargo sim                    # -> target/render/gojira_demo.wav
```

This renders original drop-C riffs at 120 bpm on a modelled electric guitar (`src/pedalkit/guitar.rs`):

1. Magma: ringing clean arpeggios (Cm9, Abmaj7, Bb, Fsus2) into the chorus, echo and room.
2. A pick scrape down the wound strings.
3. Sirius: a palm-muted chug riff with power chords, staccato stops and a pinch-harmonic squeal.
4. The Whammy: a single-note riff where every note swoops up an octave.
5. Staccato hits and a last chord left to ring, muted into the reverb tail.

Then it prints the level and stereo width of each section.

## Code layout

- `src/gojira/`: the pedal. `patch.rs` wires it up, `presets.rs` holds every number, and the other files are its blocks.
- `src/pedalkit/`: shared building blocks (effects, DSP, the Endless ABI, the entry points, the demo renderer). It's a copy of the master in [superPedal](https://github.com/matteliot/superPedal), which also carries this pedal, so don't edit it here.
- `src/host/`: the `cargo sim` demo (`score.rs` is what it plays).
