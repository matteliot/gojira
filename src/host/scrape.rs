//! A pick scrape down the wound strings, added to the guitar for the demo:
//! a falling band-passed click train with noise.

use crate::pedalkit::dsp::Biquad;
use crate::pedalkit::host::{Extra, SRU};

/// A pick dragged along the wound strings: the pick edge clicks over each
/// winding (a pulse train whose rate falls as it slows), heard through the
/// strings' bright, hissy resonance, which drops as it moves up the neck.
pub struct Scrape {
    bp: Biquad,
    i: usize,
    len: usize,
    ph: f32,
    seed: u32,
}

impl Scrape {
    const LEVEL: f32 = 0.35;

    pub fn new() -> Scrape {
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

impl Extra for Scrape {
    fn event(&mut self, _id: u32, secs: f32) {
        self.start(secs);
    }
    fn has_source(&self) -> bool {
        true
    }
    fn tick(&mut self) -> f32 {
        Scrape::tick(self)
    }
}
