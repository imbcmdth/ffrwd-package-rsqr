//! `scan`: the picture untouched, every QR code beside it as a cue row.

// The 0.10.0 world, checked in beside the crate: this module reads one frame
// per call and gains nothing from the borrowed window, while the installed
// `ffrwd/wasm` package carries only the version the manifest names.
wit_bindgen::generate!({
    path: "wit-0.10.0",
    world: "window-module",
});

use std::cell::RefCell;

use exports::ffrwd::av::window_filter::{
    Format, FramePayload, Guest, InFrame, Meta, OutFrame, Processed, StreamInfo, WindowMeta,
};
use rsqr_core::{
    heads, Cue, Instance, WindowFrame, PARAMS_SCHEMA, PIXEL_FORMAT, SCAN_WINDOW, STRIDE,
};

const NAME: &str = "scan";
const VERSION: &str = "0.1.0";

const ROWS_SCHEMA: &str = r#"{"type":"object","properties":{"text":{"type":"string"},"start_t":{"type":"number"},"end_t":{"type":"number"}},"required":["text","start_t","end_t"],"additionalProperties":false}"#;

thread_local! {
    static INSTANCE: RefCell<Instance> = RefCell::new(Instance::new(NAME));
}

struct Scan;

impl Guest for Scan {
    fn describe() -> WindowMeta {
        WindowMeta {
            meta: Meta {
                name: NAME.to_string(),
                version: VERSION.to_string(),
                params_schema: PARAMS_SCHEMA.to_string(),
                rows_schema: ROWS_SCHEMA.to_string(),
                pixel_formats: vec![PIXEL_FORMAT.to_string()],
                sample_formats: vec![],
                sample_rates: vec![],
                channel_counts: vec![],
                // The cues mint an untagged track: the language a call could
                // name is not declared here yet.
                rows_language: vec![],
            },
            window: SCAN_WINDOW,
            stride: STRIDE,
            // A run outlives the call it started in, so a call answers out of
            // what earlier calls left behind.
            pure: false,
            // One output per frame consumed, each at that frame's own pts.
            one_to_one: true,
            reads_rows: false,
            forwards_rows: false,
            inputs: 1,
        }
    }

    fn init(format: Format, stream_info: StreamInfo, params: String) -> Result<(), String> {
        INSTANCE.with_borrow_mut(|instance| {
            let Format::Video(video) = format else {
                return Err(instance.not_video());
            };
            instance.open(
                video.width,
                video.height,
                &video.pix_fmt,
                (stream_info.time_base.num, stream_info.time_base.den),
                &params,
            )
        })
    }

    fn set_params(params: String) -> Result<(), String> {
        INSTANCE.with_borrow(|instance| instance.read_params(&params))
    }

    fn process(frames: Vec<InFrame>, _trailing: Vec<String>, last: bool) -> Processed {
        INSTANCE.with_borrow_mut(|instance| {
            let mut out: Vec<OutFrame> = Vec::new();
            for head in heads(frames.len(), last) {
                // Only the frame the call speaks for is read. The frames a
                // sighting is carried back over are reached by their
                // timestamps, so their pixels are never asked for.
                let window = [WindowFrame {
                    pts: frames[head].pts,
                    frame: &frames[head].frame,
                }];
                let seen = instance.read(&window);
                let time = instance.seconds(frames[head].pts);
                let cues = instance.runs.credit(time, &seen.sightings);
                out.push(OutFrame {
                    pts: frames[head].pts,
                    // The picture leaves as it arrived; the host copies nothing.
                    frame: FramePayload::Same,
                    rows: cues.iter().map(Cue::row).collect(),
                });
            }
            if !last {
                return Processed {
                    frames: out,
                    trailing: vec![],
                };
            }

            // Every frame has left, so a code still on screen closes here.
            let closing: Vec<String> = instance.runs.flush().iter().map(Cue::row).collect();
            match out.last_mut() {
                Some(final_frame) => {
                    final_frame.rows.extend(closing);
                    Processed {
                        frames: out,
                        trailing: vec![],
                    }
                }
                None => Processed {
                    frames: out,
                    trailing: closing,
                },
            }
        })
    }
}

export!(Scan);
