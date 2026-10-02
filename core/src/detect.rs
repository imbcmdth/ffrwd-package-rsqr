// The QR lookup over rgba pixels, the mosaic that redacts a code, and the row
// `scan` writes for each code it sees.

use std::collections::VecDeque;

/// Bytes per pixel. The wire carries rgba, so a frame is read where it lies
/// and only the luma the decoder wants is derived from it.
pub const CHANNELS: usize = 4;

/// How many frames `mosaic_codes` sees in one call: a frame is redacted by
/// every code found in it or in the 14 frames after it, so a code is covered
/// from before the decoder first read it. 15 frames is half a second at
/// 30fps: long enough to cover the frames the decoder missed a code in, short
/// enough that the look-ahead stays cheap.
pub const WINDOW: u32 = 15;
pub const STRIDE: u32 = 1;

/// How many frames in a row a code may go unread and still be the same
/// sighting to `scan`, which is the gap the window above heals for
/// `mosaic_codes`: the two exports agree on what one appearance is.
pub const GAP: u32 = WINDOW - 1;

/// How many times one frame is searched. Each pass paints out what it found
/// before the next looks, so the cost is one pass per round of codes plus one
/// that fails.
const MAX_PASSES_PER_FRAME: usize = 8;

/// A rectangle of the picture, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoxRect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// One code read off one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub text: String,
    pub bbox: BoxRect,
}

/// One code in view on one frame, as the row `scan` emits: the time the
/// sighting it belongs to began, which names that sighting, how many
/// sightings began before it, and its payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Sighting {
    pub start_t: f64,
    pub id: u64,
    pub text: String,
}

impl Sighting {
    /// The JSON row, byte for byte what `JSON.stringify` writes for it, so
    /// `ffrwd/jsqr` writes the same rows.
    pub fn row(&self) -> String {
        format!(
            r#"{{"start_t":{},"id":{},"text":{}}}"#,
            time(self.start_t),
            self.id,
            serde_json::to_string(&self.text).expect("a string is always JSON")
        )
    }
}

/// A time as JSON carries it: every digit that round-trips, and no trailing
/// fraction on a whole number.
fn time(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Each payload one frame shows, once, whatever order the decoder met them
/// in: sorted, so two decoders reading the same frame name them alike.
pub fn payloads(codes: &[Found]) -> Vec<&str> {
    let mut texts: Vec<&str> = codes.iter().map(|code| code.text.as_str()).collect();
    // UTF-16 order, which is how JavaScript sorts strings.
    texts.sort_unstable_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    texts.dedup();
    texts
}

/// One rgba pixel's luma, by the BT.709 weights, rounded to the byte the
/// decoder reads. Alpha takes no part.
pub fn greyscale(rgba: &[u8], width: usize, x: usize, y: usize) -> u8 {
    let i = (y * width + x) * CHANNELS;
    let luma = 0.2126 * rgba[i] as f64 + 0.7152 * rgba[i + 1] as f64 + 0.0722 * rgba[i + 2] as f64;
    luma.round() as u8
}

/// The axis-aligned box around a code's four corners, clipped to the frame.
pub fn bounding_box(corners: [(f64, f64); 4], width: usize, height: usize) -> BoxRect {
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for (cx, cy) in corners {
        left = left.min(cx);
        top = top.min(cy);
        right = right.max(cx);
        bottom = bottom.max(cy);
    }
    let x = left.floor().max(0.0) as usize;
    let y = top.floor().max(0.0) as usize;
    let span = |far: f64, limit: usize, near: usize| -> usize {
        let cut = (far.ceil().max(0.0) as usize).min(limit);
        cut.saturating_sub(near).max(1)
    };
    BoxRect {
        x,
        y,
        w: span(right, width, x),
        h: span(bottom, height, y),
    }
}

/// The mosaic block for a box that wide.
///
/// A QR code carries up to 30% error correction and reads straight through a
/// blur, so the block has to be larger than the code's own module or the
/// redaction is decorative. Eight modules across the box is the fewest a
/// version-1 code reaches, so a block an eighth of the box wide swallows at
/// least one module whatever version it is.
pub fn block_size(box_width: usize) -> usize {
    (box_width / 8).max(2)
}

/// Pixelates one box of an rgba frame in place, block by block.
pub fn mosaic_box(rgba: &mut [u8], width: usize, height: usize, bbox: &BoxRect) {
    let size = block_size(bbox.w);
    let right = width.min(bbox.x + bbox.w);
    let bottom = height.min(bbox.y + bbox.h);
    let mut by = bbox.y;
    while by < bottom {
        let block_bottom = bottom.min(by + size);
        let mut bx = bbox.x;
        while bx < right {
            let block_right = right.min(bx + size);
            let (mut r, mut g, mut b, mut count) = (0u64, 0u64, 0u64, 0u64);
            for y in by..block_bottom {
                for x in bx..block_right {
                    let i = (y * width + x) * CHANNELS;
                    r += rgba[i] as u64;
                    g += rgba[i + 1] as u64;
                    b += rgba[i + 2] as u64;
                    count += 1;
                }
            }
            if count > 0 {
                let average = |sum: u64| (sum as f64 / count as f64).round() as u8;
                let (ar, ag, ab) = (average(r), average(g), average(b));
                for y in by..block_bottom {
                    for x in bx..block_right {
                        let i = (y * width + x) * CHANNELS;
                        rgba[i] = ar;
                        rgba[i + 1] = ag;
                        rgba[i + 2] = ab;
                    }
                }
            }
            bx += size;
        }
        by += size;
    }
}

/// Fills a box of an rgba buffer with white, so the next pass cannot see it.
fn paint_out(rgba: &mut [u8], width: usize, bbox: &BoxRect) {
    for y in bbox.y..bbox.y + bbox.h {
        let row = y * width * CHANNELS;
        for x in bbox.x..bbox.x + bbox.w {
            let i = row + x * CHANNELS;
            rgba[i] = 255;
            rgba[i + 1] = 255;
            rgba[i + 2] = 255;
        }
    }
}

/// Every code one look at the frame reads.
fn decode_pass(rgba: &[u8], width: usize, height: usize) -> Vec<Found> {
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
        greyscale(rgba, width, x, y)
    });
    let mut found = Vec::new();
    for grid in prepared.detect_grids() {
        let Ok((_meta, text)) = grid.decode() else {
            continue;
        };
        let corners = grid.bounds.map(|point| (point.x as f64, point.y as f64));
        found.push(Found {
            text,
            bbox: bounding_box(corners, width, height),
        });
    }
    found
}

/// Every QR code in one rgba frame, as `{ text, bbox }`.
///
/// A pass reads what it can see, and what it found is painted out before the
/// frame is searched again - so a code the locator only reaches once its
/// neighbour is gone is still reached. The frame handed in is never written
/// to: a copy is taken only once a code is found, because the frame handed in
/// may be the frame handed back.
pub fn detect_codes(frame: &[u8], width: usize, height: usize) -> Vec<Found> {
    let mut scratch: Option<Vec<u8>> = None;
    let mut found: Vec<Found> = Vec::new();
    for _ in 0..MAX_PASSES_PER_FRAME {
        let source = scratch.as_deref().unwrap_or(frame);
        let fresh: Vec<Found> = decode_pass(source, width, height)
            .into_iter()
            .filter(|one| !found.contains(one))
            .collect();
        if fresh.is_empty() {
            break;
        }
        let painted = scratch.get_or_insert_with(|| frame.to_vec());
        for one in fresh {
            paint_out(painted, width, &one.bbox);
            found.push(one);
        }
    }
    found
}

/// Runs detection over a frame once per timestamp, however often asked.
///
/// A window of 15 with a stride of 1 hands the same frame in 15 times, so this
/// is what keeps the work at one pass per frame rather than fifteen.
pub struct DetectionCache {
    keep: usize,
    entries: VecDeque<(i64, Vec<Found>)>,
    decoded: usize,
}

impl Default for DetectionCache {
    fn default() -> Self {
        DetectionCache::new(WINDOW as usize * 2)
    }
}

impl DetectionCache {
    pub fn new(keep: usize) -> Self {
        DetectionCache {
            keep: keep.max(1),
            entries: VecDeque::new(),
            decoded: 0,
        }
    }

    /// How many frames were actually put through the decoder.
    pub fn decoded(&self) -> usize {
        self.decoded
    }

    /// Whether this timestamp's codes are already held, so `codes_for` would
    /// answer without reading the frame.
    pub fn holds(&self, pts: i64) -> bool {
        self.entries.iter().any(|(seen, _)| *seen == pts)
    }

    pub fn codes_for(&mut self, pts: i64, frame: &[u8], width: usize, height: usize) -> &[Found] {
        let at = match self.entries.iter().position(|(seen, _)| *seen == pts) {
            Some(at) => at,
            None => {
                let codes = detect_codes(frame, width, height);
                self.decoded += 1;
                self.entries.push_back((pts, codes));
                // The oldest entry is the first inserted, and the window only
                // moves forward, so dropping from the front is dropping what
                // has left it.
                while self.entries.len() > self.keep {
                    self.entries.pop_front();
                }
                self.entries.len() - 1
            }
        };
        &self.entries[at].1
    }
}
