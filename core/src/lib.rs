//! The detection both exports share, free of any wasm binding: the QR lookup
//! over rgba pixels, the mosaic that redacts a code, the row a sighting is
//! written as, and the instance `mosaic_codes` answers out of.

mod detect;
mod instance;

pub use detect::{
    block_size, bounding_box, detect_codes, greyscale, mosaic_box, payloads, BoxRect,
    DetectionCache, Found, Sighting, CHANNELS, GAP, STRIDE, WINDOW,
};
pub use instance::{Instance, PIXEL_FORMAT};
