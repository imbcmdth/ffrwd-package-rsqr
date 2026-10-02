//! `mosaic_codes`: the picture with every QR code pixelated, redacted by the
//! codes found in it and in the frames after it.

use ffrwd_node::{Bound, Init, Input, NoParams, Node, Out, Output, Result, Shape, Tick};
use rsqr_core::{mosaic_box, Instance, PIXEL_FORMAT, STRIDE, WINDOW};

struct MosaicCodes {
    v: u32,
    instance: Instance,
}

impl Node for MosaicCodes {
    const NAME: &'static str = "mosaic_codes";
    const VERSION: &'static str = "0.2.0";
    type Params = NoParams;

    fn shape(_: &NoParams, _: &Bound) -> Result<Shape> {
        Ok(Shape::new()
            .input(
                Input::video("v")
                    .clock()
                    .window(WINDOW, STRIDE)
                    .pixel_formats(&[PIXEL_FORMAT]),
            )
            .output(Output::like("v"))
            // The decode cache only saves work; each frame is redacted out of
            // its own window.
            .pure()
            .one_to_one())
    }

    fn init(_: NoParams, init: &Init) -> Result<MosaicCodes> {
        let v = init.stream("v")?;
        let video = v.video_format().ok_or("`v` is a video input")?;
        Ok(MosaicCodes {
            v: v.id,
            instance: Instance::new(video.width, video.height),
        })
    }

    fn process(&mut self, tick: &Tick, out: &mut Out) -> Result<()> {
        let window = tick.frames(self.v);
        let Some(head) = window.first() else {
            return Ok(());
        };
        let pts: Vec<i64> = window.iter().map(|frame| frame.pts).collect();
        let boxes = self
            .instance
            .boxes(&pts, |i| tick.fetch(self.v, window[i].index));
        if boxes.is_empty() {
            return Ok(out.pass("v", self.v, head)?);
        }
        let mut redacted = tick.fetch(self.v, head.index);
        for bbox in &boxes {
            mosaic_box(
                &mut redacted,
                self.instance.width(),
                self.instance.height(),
                bbox,
            );
        }
        Ok(out.frame("v", head.pts, head.duration, redacted)?)
    }
}

ffrwd_node::export!(MosaicCodes);

#[cfg(test)]
mod tests {
    use super::*;
    use ffrwd_node::mock::Harness;
    use ffrwd_node::{BoundStream, Payload, Rational};
    use qrcode::{Color, EcLevel, QrCode};
    use rsqr_core::{detect_codes, CHANNELS};

    const SIDE: usize = 120;

    fn blank() -> Vec<u8> {
        vec![255u8; SIDE * SIDE * CHANNELS]
    }

    // Black and white columns a pixel wide: a picture a mosaic cannot leave
    // as it was, and no code.
    fn stripes() -> Vec<u8> {
        let mut rgba = blank();
        for pixel in rgba.as_chunks_mut::<CHANNELS>().0.iter_mut().step_by(2) {
            pixel[..3].fill(0);
        }
        rgba
    }

    // One code painted dark on white, 3 pixels a module, inside its quiet
    // zone.
    fn coded() -> Vec<u8> {
        let mut rgba = blank();
        let code = QrCode::with_error_correction_level("ffrwd", EcLevel::M).unwrap();
        let modules = code.width();
        for (at, color) in code.to_colors().into_iter().enumerate() {
            if color != Color::Dark {
                continue;
            }
            let (x0, y0) = ((at % modules + 4) * 3, (at / modules + 4) * 3);
            for y in y0..y0 + 3 {
                for x in x0..x0 + 3 {
                    rgba[(y * SIDE + x) * CHANNELS..][..3].fill(0);
                }
            }
        }
        rgba
    }

    fn harness() -> Harness<MosaicCodes> {
        let v = BoundStream::video(
            "v",
            0,
            SIDE as u32,
            SIDE as u32,
            "rgba",
            Rational::new(1, 30),
        );
        Harness::new("", vec![v]).unwrap()
    }

    #[test]
    fn the_output_follows_the_picture_over_a_sliding_window() {
        let mosaic = harness();
        let shape = mosaic.shape();
        let v = shape.find_input("v").unwrap();
        assert_eq!((v.window, v.stride), (WINDOW, STRIDE));
        let like = shape.find_output("v").unwrap().like.as_ref().unwrap();
        assert_eq!(like.port.as_deref(), Some("v"));
        assert!(shape.pure && shape.one_to_one);
    }

    #[test]
    fn a_frame_is_redacted_by_a_code_later_in_its_window() {
        let mut mosaic = harness();
        let tick = mosaic.tick(0).frame(0, 0, stripes()).frame(0, 1, coded());
        let emitted = mosaic.process(&tick).unwrap();
        let [Payload::Frame { pts: 0, data, .. }] = emitted.on("v")[..] else {
            panic!("the head frame was not redrawn");
        };
        assert_ne!(data, &stripes());
    }

    #[test]
    fn a_window_with_no_code_hands_its_head_back_uncopied() {
        let mut mosaic = harness();
        let tick = mosaic.tick(0).frame(0, 0, blank()).frame(0, 1, blank());
        let emitted = mosaic.process(&tick).unwrap();
        assert!(matches!(
            emitted.on("v")[..],
            [Payload::Same {
                pts: 0,
                id: 0,
                index: 0,
                ..
            }]
        ));
    }

    #[test]
    fn the_redacted_code_no_longer_reads() {
        let mut mosaic = harness();
        let tick = mosaic.tick(0).frame(0, 0, coded()).last();
        let emitted = mosaic.process(&tick).unwrap();
        let [Payload::Frame { data, .. }] = emitted.on("v")[..] else {
            panic!("the coded frame was not redrawn");
        };
        assert!(!detect_codes(&coded(), SIDE, SIDE).is_empty());
        assert!(detect_codes(data, SIDE, SIDE).is_empty());
    }
}
