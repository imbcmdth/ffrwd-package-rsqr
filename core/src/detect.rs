// The QR lookup over rgba pixels, the mosaic that redacts a code, and the
// runs that turn per-frame sightings into cues.

use std::collections::VecDeque;

/// Bytes per pixel. The wire carries rgba, so a frame is read where it lies
/// and only the luma the decoder wants is derived from it.
pub const CHANNELS: usize = 4;

/// How many frames a code is carried back over, and how many `mosaic_codes`
/// therefore sees in one call. The host reads the window off `describe` BEFORE
/// `init`, and never asks again - so this is the module's own constant and
/// cannot be a parameter of the SQL call. 15 frames is half a second at 30fps:
/// long enough to carry a code back over the frames the decoder missed it in,
/// short enough that the look-ahead stays cheap.
pub const WINDOW: u32 = 15;
pub const STRIDE: u32 = 1;

/// How many frames `scan` sees in one call. It reads only the first of them:
/// the frames a sighting is carried back over are reached by the timestamps it
/// kept as they passed, never by their pixels. Two rather than one because a
/// window of one comes out even, and the final call would then carry no frame
/// for the closing cues to ride.
pub const SCAN_WINDOW: u32 = 2;

/// How many times one frame is searched. Each pass paints out what it found
/// before the next looks, so the cost is one pass per round of codes plus one
/// that fails.
const MAX_PASSES_PER_FRAME: usize = 8;

/// The frame interval assumed until two frames have arrived.
pub const ASSUMED_INTERVAL: f64 = 1.0 / 30.0;

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

/// One code's span on screen, as the row `scan` emits.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub text: String,
    pub start_t: f64,
    pub end_t: f64,
}

impl Cue {
    /// The NDJSON row this cue rides out on.
    pub fn row(&self) -> String {
        format!(
            r#"{{"text":{},"start_t":{},"end_t":{}}}"#,
            serde_json::to_string(&self.text).expect("a string is always JSON"),
            time(self.start_t),
            time(self.end_t)
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

/// What one window saw: each payload against the time of the LAST frame in
/// the window it was really seen in, in the order the window met them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sightings(Vec<(String, f64)>);

impl Sightings {
    pub fn new() -> Self {
        Sightings(Vec::new())
    }

    /// Keeps the later of this sighting and any already held for the payload.
    pub fn note(&mut self, text: &str, time: f64) {
        match self.0.iter_mut().find(|(held, _)| held == text) {
            Some((_, held)) => {
                if time > *held {
                    *held = time;
                }
            }
            None => self.0.push((text.to_string(), time)),
        }
    }

    pub fn get(&self, text: &str) -> Option<f64> {
        self.0
            .iter()
            .find(|(held, _)| held == text)
            .map(|(_, time)| *time)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, f64)> {
        self.0.iter().map(|(text, time)| (text.as_str(), *time))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<(String, f64)> for Sightings {
    fn from_iter<I: IntoIterator<Item = (String, f64)>>(items: I) -> Self {
        let mut sightings = Sightings::new();
        for (text, time) in items {
            sightings.note(&text, time);
        }
        sightings
    }
}

/// One payload's unbroken stretch of credited frames.
struct Run {
    start_time: f64,
    last_time: f64,
    /// Frames credited since the last one this code was seen in.
    missed: u32,
    /// The interval in force at the first of those - the frame the run ends
    /// on, which is not the frame its cue comes out on.
    closing_interval: f64,
}

/// The runs a code makes across the frames it is credited to.
///
/// A code is credited back over the WINDOW frames before the one it was read
/// in, so a code the decoder only catches late still covers the frames before
/// it and the flicker a per-frame decoder produces closes up. That reach back
/// is arithmetic: the timestamps of those frames are held as they pass, and
/// their pixels are never asked for.
///
/// A run is the stretch of frames credited with one payload without a break,
/// which heals any gap the window can span, and it closes into one cue: the
/// first frame credited, through the last frame the code was really seen in.
/// A run ENDS on the frame after its last sighting; nothing ahead being known,
/// that is only certain a window later, which is when its cue comes out.
pub struct CodeRuns {
    runs: Vec<(String, Run)>,
    interval: f64,
    previous_time: Option<f64>,
    /// The times of the last WINDOW frames, this one last. The first of them
    /// is where a run starting now reaches back to.
    recent: VecDeque<f64>,
}

impl Default for CodeRuns {
    fn default() -> Self {
        CodeRuns::new()
    }
}

impl CodeRuns {
    pub fn new() -> Self {
        CodeRuns {
            runs: Vec::new(),
            interval: ASSUMED_INTERVAL,
            previous_time: None,
            recent: VecDeque::new(),
        }
    }

    /// One frame's credit. Returns the cues of the runs now known to have
    /// ended - each a window ago, since that is how long it takes to be sure.
    pub fn credit(&mut self, time: f64, sightings: &Sightings) -> Vec<Cue> {
        if let Some(previous) = self.previous_time {
            if time > previous {
                self.interval = time - previous;
            }
        }
        self.previous_time = Some(time);

        self.recent.push_back(time);
        while self.recent.len() > WINDOW as usize {
            self.recent.pop_front();
        }
        let reaches_back_to = *self.recent.front().expect("this frame is in it");

        for (text, last_seen) in sightings.iter() {
            match self.runs.iter_mut().find(|(held, _)| held == text) {
                Some((_, run)) => {
                    run.last_time = run.last_time.max(last_seen);
                    run.missed = 0;
                }
                None => self.runs.push((
                    text.to_string(),
                    Run {
                        start_time: reaches_back_to,
                        last_time: last_seen,
                        missed: 0,
                        closing_interval: self.interval,
                    },
                )),
            }
        }

        let interval = self.interval;
        let mut cues = Vec::new();
        self.runs.retain_mut(|(text, run)| {
            if sightings.get(text).is_some() {
                return true;
            }
            if run.missed == 0 {
                run.closing_interval = interval;
            }
            run.missed += 1;
            if run.missed < WINDOW {
                return true;
            }
            cues.push(cue(text, run));
            false
        });
        cues
    }

    /// The cues of every run the stream ended before, which then close. A run
    /// the window had not finished missing ended before the stream did, so it
    /// comes out ahead of the ones still on screen at the end.
    pub fn flush(&mut self) -> Vec<Cue> {
        let interval = self.interval;
        let mut held: Vec<(String, Run)> = std::mem::take(&mut self.runs);
        for (_, run) in &mut held {
            if run.missed == 0 {
                run.closing_interval = interval;
            }
        }
        held.sort_by(|(_, a), (_, b)| ends_at(a).total_cmp(&ends_at(b)));
        held.iter().map(|(text, run)| cue(text, run)).collect()
    }
}

// When a run ended, for the order the cues come out in. A run still on screen
// ends with the stream, after every run that ran out before it.
fn ends_at(run: &Run) -> f64 {
    if run.missed == 0 {
        f64::INFINITY
    } else {
        run.last_time
    }
}

// A run as one cue. A code caught in a single frame would span nothing, so
// such a cue is given one frame's width.
fn cue(text: &str, run: &Run) -> Cue {
    let end = if run.last_time > run.start_time {
        run.last_time
    } else {
        run.start_time + run.closing_interval
    };
    Cue {
        text: text.to_string(),
        start_t: run.start_time,
        end_t: end,
    }
}
