//! The detection both exports share, free of any wasm binding: the QR lookup
//! over rgba pixels, the mosaic that redacts a code, the runs that turn
//! per-frame sightings into cues, and the instance state a call answers out
//! of.

mod detect;
mod instance;

pub use detect::{
    block_size, bounding_box, detect_codes, greyscale, mosaic_box, BoxRect, CodeRuns, Cue,
    DetectionCache, Found, Sightings, ASSUMED_INTERVAL, CHANNELS, SCAN_WINDOW, STRIDE, WINDOW,
};
pub use instance::{heads, Instance, WindowFrame, Windowed, PARAMS_SCHEMA, PIXEL_FORMAT};
