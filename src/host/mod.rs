//! Host-only demo: `cargo sim [OUT.wav]` plays `score` on the modelled
//! guitar, with a pick scrape mixed in (`scrape`), through the patch (see
//! `pedalkit::host`).

mod scrape;
mod score;

use crate::entry::ENTRY;
use crate::pedalkit::host::{self, Options};

pub fn main() {
    let out = host::out_path("target/render/gojira_demo.wav");
    let (events, total) = score::score();
    let o = Options { first: "magma (clean + space)", input_trim: 0.6, working: 16 };
    let r = host::render_extra(&ENTRY, events, total, o, &mut scrape::Scrape::new());
    host::finish(&ENTRY, r, &out);
}
