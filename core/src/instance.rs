// What both exports share at the wasm boundary: the opened instance, the one
// window each call is read over, and which frames a call speaks for.

use std::ops::Range;

use serde_json::Value;

use crate::detect::{BoxRect, CodeRuns, DetectionCache, Sightings};

pub const PIXEL_FORMAT: &str = "rgba";

// Neither export takes a value parameter: the window is settled at describe
// time, before `init` is ever called, so there is nothing left for a call to
// say.
pub const PARAMS_SCHEMA: &str = r#"{"type":"object","properties":{},"additionalProperties":false}"#;

/// One input payload of a window: its timestamp and its pixels.
pub struct WindowFrame<'a> {
    pub pts: i64,
    pub frame: &'a [u8],
}

/// One window read: what it saw, and where.
pub struct Windowed {
    /// Each payload against the time of the LAST frame in the window it was
    /// really seen in - the end of its cue.
    pub sightings: Sightings,
    /// Every distinct box anywhere in the window, which is what the frame the
    /// window heads has to have redacted out of it.
    pub boxes: Vec<BoxRect>,
}

/// One opened instance: the geometry, the time base, and the running state.
pub struct Instance {
    name: &'static str,
    width: usize,
    height: usize,
    time_base: (i32, i32),
    cache: DetectionCache,
    pub runs: CodeRuns,
}

impl Instance {
    pub fn new(name: &'static str) -> Self {
        Instance {
            name,
            width: 0,
            height: 0,
            time_base: (1, 1),
            cache: DetectionCache::default(),
            runs: CodeRuns::new(),
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Settles the geometry and the time base, and clears what earlier frames
    /// left behind.
    pub fn open(
        &mut self,
        width: u32,
        height: u32,
        pix_fmt: &str,
        time_base: (i32, i32),
        params: &str,
    ) -> Result<(), String> {
        if pix_fmt != PIXEL_FORMAT {
            return Err(format!(
                "{} filters {PIXEL_FORMAT}, opened for {pix_fmt}",
                self.name
            ));
        }
        self.read_params(params)?;
        self.width = width as usize;
        self.height = height as usize;
        self.time_base = time_base;
        self.cache = DetectionCache::default();
        self.runs = CodeRuns::new();
        Ok(())
    }

    /// The message for an instance opened on the wrong kind of stream.
    pub fn not_video(&self) -> String {
        format!("{} filters video, opened for audio", self.name)
    }

    /// Checks the parameters, leaving the previous ones in force on refusal.
    pub fn read_params(&self, params: &str) -> Result<(), String> {
        let text = params.trim();
        if text.is_empty() {
            return Ok(());
        }
        let parsed: Value = serde_json::from_str(text)
            .map_err(|error| format!("{} cannot read its params: {error}", self.name))?;
        let Value::Object(named) = parsed else {
            return Err(format!("{} takes its params as a JSON object", self.name));
        };
        if !named.is_empty() {
            let names: Vec<&str> = named.keys().map(String::as_str).collect();
            return Err(format!(
                "{} takes no parameters, and was given {}",
                self.name,
                names.join(", ")
            ));
        }
        Ok(())
    }

    /// A timestamp in the stream's own base, as seconds.
    pub fn seconds(&self, pts: i64) -> f64 {
        (pts as f64 * self.time_base.0 as f64) / self.time_base.1 as f64
    }

    /// One window read, decoding each timestamp only the first time it is met.
    pub fn read(&mut self, window: &[WindowFrame]) -> Windowed {
        let mut sightings = Sightings::new();
        let mut boxes: Vec<BoxRect> = Vec::new();
        for input in window {
            let time = self.seconds(input.pts);
            for code in self
                .cache
                .codes_for(input.pts, input.frame, self.width, self.height)
            {
                sightings.note(&code.text, time);
                if !boxes.contains(&code.bbox) {
                    boxes.push(code.bbox);
                }
            }
        }
        Windowed { sightings, boxes }
    }
}

/// The frames one call speaks for: its first, or every leftover on the last.
///
/// A regular call consumes one frame and sees the `window` frames from it on.
/// The final call is handed whatever the last stride left over and consumes
/// all of it, so it speaks for every one - each over the frames still ahead of
/// it, a window that shortens to nothing at the end of the stream.
pub fn heads(frames: usize, last: bool) -> Range<usize> {
    0..if last { frames } else { frames.min(1) }
}
