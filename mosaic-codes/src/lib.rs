//! `mosaic_codes`: the picture with every QR code pixelated, no rows.

wit_bindgen::generate!({
    path: "wit",
    world: "window-module",
});

use std::cell::RefCell;

use exports::ffrwd::av::window_filter::{
    Format, FramePayload, Guest, InWindow, Meta, OutFrame, Processed, StreamInfo, WindowMeta,
};
use rsqr_core::{heads, mosaic_box, Instance, PARAMS_SCHEMA, PIXEL_FORMAT, STRIDE, WINDOW};

const NAME: &str = "mosaic_codes";
const VERSION: &str = "0.1.0";

thread_local! {
    static INSTANCE: RefCell<Instance> = RefCell::new(Instance::new(NAME));
}

struct MosaicCodes;

impl Guest for MosaicCodes {
    fn describe() -> WindowMeta {
        WindowMeta {
            meta: Meta {
                name: NAME.to_string(),
                version: VERSION.to_string(),
                params_schema: PARAMS_SCHEMA.to_string(),
                rows_schema: String::new(),
                pixel_formats: vec![PIXEL_FORMAT.to_string()],
                sample_formats: vec![],
                sample_rates: vec![],
                channel_counts: vec![],
                rows_language: vec![],
            },
            window: WINDOW,
            stride: STRIDE,
            // Each frame is redacted out of its own window and nothing else,
            // so a call answers out of what it was handed.
            pure: true,
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

    fn process(window: &InWindow, _trailing: Vec<String>, last: bool) -> Processed {
        INSTANCE.with_borrow_mut(|instance| {
            let len = window.len() as usize;
            let mut out: Vec<OutFrame> = Vec::new();
            for head in heads(len, last) {
                let pts: Vec<i64> = (head..len).map(|i| window.pts(i as u32)).collect();
                // Only timestamps this instance has not decoded yet have
                // their bytes copied in; the rest answer out of the cache.
                let boxes = instance
                    .read_fetching(&pts, |i| window.fetch((head + i) as u32))
                    .boxes;
                let payload = if boxes.is_empty() {
                    // Nothing to redact, so the frame passes through uncopied.
                    FramePayload::Same
                } else {
                    // Fetched to be written on: `fetch` hands over the copy
                    // the redaction happens in.
                    let mut redacted = window.fetch(head as u32);
                    for bbox in &boxes {
                        mosaic_box(&mut redacted, instance.width(), instance.height(), bbox);
                    }
                    FramePayload::New(redacted)
                };
                out.push(OutFrame {
                    pts: pts[0],
                    frame: payload,
                    rows: vec![],
                });
            }
            Processed {
                frames: out,
                trailing: vec![],
            }
        })
    }
}

export!(MosaicCodes);
