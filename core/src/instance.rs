// What `mosaic_codes` keeps between calls: the geometry, and the codes of the
// frames it has already decoded.

use crate::detect::{BoxRect, DetectionCache};

pub const PIXEL_FORMAT: &str = "rgba";

/// One opened instance.
pub struct Instance {
    width: usize,
    height: usize,
    cache: DetectionCache,
}

impl Instance {
    pub fn new(width: u32, height: u32) -> Self {
        Instance {
            width: width as usize,
            height: height as usize,
            cache: DetectionCache::default(),
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Every distinct box anywhere in one window, which is what the frame the
    /// window heads has to have redacted out of it. `pts` is every timestamp
    /// of the window, oldest first, and `fetch` copies index `i`'s bytes in. A
    /// timestamp met before is answered out of the cache and its bytes are
    /// never asked for.
    pub fn boxes(&mut self, pts: &[i64], mut fetch: impl FnMut(usize) -> Vec<u8>) -> Vec<BoxRect> {
        let mut boxes: Vec<BoxRect> = Vec::new();
        for (i, &stamp) in pts.iter().enumerate() {
            // A held timestamp never reaches the decoder, so the empty slice
            // stands in for bytes that were never copied.
            let bytes = if self.cache.holds(stamp) {
                Vec::new()
            } else {
                fetch(i)
            };
            for code in self.cache.codes_for(stamp, &bytes, self.width, self.height) {
                if !boxes.contains(&code.bbox) {
                    boxes.push(code.bbox);
                }
            }
        }
        boxes
    }
}
