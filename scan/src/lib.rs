//! `scan`: a row per frame for each QR code in view, every row of one
//! sighting naming it by the time it began.

use ffrwd_node::{Bound, Init, Input, NoParams, Node, Out, Output, Result, Shape, Spans, Tick};
use rsqr_core::{detect_codes, payloads, Sighting, GAP, PIXEL_FORMAT};

const ROW_SCHEMA: &str = r#"{"type":"object","properties":{"start_t":{"type":"number"},"id":{"type":"integer"},"text":{"type":"string"}},"required":["start_t","id","text"],"additionalProperties":false}"#;

struct Scan {
    v: u32,
    width: usize,
    height: usize,
    sightings: Spans<String>,
}

impl Node for Scan {
    const NAME: &'static str = "scan";
    const VERSION: &'static str = "0.2.0";
    type Params = NoParams;

    fn shape(_: &NoParams, _: &Bound) -> Result<Shape> {
        Ok(Shape::new()
            .input(Input::video("v").clock().pixel_formats(&[PIXEL_FORMAT]))
            .output(Output::rows("codes").schema_json(ROW_SCHEMA)))
    }

    fn init(_: NoParams, init: &Init) -> Result<Scan> {
        let v = init.stream("v")?;
        let video = v.video_format().ok_or("`v` is a video input")?;
        Ok(Scan {
            v: v.id,
            width: video.width as usize,
            height: video.height as usize,
            sightings: Spans::new().gap(GAP as u64),
        })
    }

    fn process(&mut self, tick: &Tick, out: &mut Out) -> Result<()> {
        let Some(frame) = tick.frame(self.v) else {
            return Ok(());
        };
        self.sightings.tick(tick.time_base().seconds(frame.pts));
        let pixels = tick.fetch(self.v, frame.index);
        for text in payloads(&detect_codes(&pixels, self.width, self.height)) {
            let sighting = self.sightings.see(text.to_owned());
            let row = Sighting {
                start_t: sighting.start_t,
                id: sighting.number,
                text: text.to_owned(),
            };
            out.message("codes", frame.pts, row.row().into_bytes())?;
        }
        Ok(())
    }
}

ffrwd_node::export!(Scan);

#[cfg(test)]
mod tests {
    use super::*;
    use ffrwd_node::mock::Harness;
    use ffrwd_node::{BoundStream, Rational};
    use qrcode::{Color, EcLevel, QrCode};
    use rsqr_core::CHANNELS;

    const WIDTH: usize = 240;
    const HEIGHT: usize = 120;
    const FPS: i64 = 30;

    // Codes painted dark on white at `at`, each `scale` pixels a module, with
    // the quiet zone a decoder needs around it.
    fn frame(codes: &[(&str, usize, usize, usize)]) -> Vec<u8> {
        let mut rgba = vec![255u8; WIDTH * HEIGHT * CHANNELS];
        for &(text, scale, left, top) in codes {
            let code = QrCode::with_error_correction_level(text, EcLevel::M).unwrap();
            let modules = code.width();
            for (at, color) in code.to_colors().into_iter().enumerate() {
                if color != Color::Dark {
                    continue;
                }
                let (x0, y0) = (
                    left + (at % modules + 4) * scale,
                    top + (at / modules + 4) * scale,
                );
                for y in y0..y0 + scale {
                    let row = (y * WIDTH + x0) * CHANNELS;
                    rgba[row..row + scale * CHANNELS].fill(0);
                    for x in 0..scale {
                        rgba[row + x * CHANNELS + 3] = 255;
                    }
                }
            }
        }
        rgba
    }

    fn harness() -> Harness<Scan> {
        let v = BoundStream::video(
            "v",
            0,
            WIDTH as u32,
            HEIGHT as u32,
            "rgba",
            Rational::new(1, FPS as i32),
        );
        Harness::new("", vec![v]).unwrap()
    }

    /// Every row `scan` writes over `count` frames, as (frame, row). `seen(k)`
    /// names the codes painted on frame k.
    fn run(
        count: i64,
        seen: impl Fn(i64) -> Vec<(&'static str, usize, usize, usize)>,
    ) -> Vec<(i64, String)> {
        let mut scan = harness();
        let mut rows = Vec::new();
        for k in 0..count {
            let mut tick = scan.tick(k).frame(0, k, frame(&seen(k)));
            if k == count - 1 {
                tick = tick.last();
            }
            rows.extend(scan.process(&tick).unwrap().messages("codes"));
        }
        rows
    }

    fn row(start: i64, id: u64, text: &str) -> String {
        Sighting {
            start_t: start as f64 / FPS as f64,
            id,
            text: text.to_owned(),
        }
        .row()
    }

    const A: (&str, usize, usize, usize) = ("ffrwd", 3, 0, 0);
    const B: (&str, usize, usize, usize) = ("https://want.video", 2, 120, 0);

    #[test]
    fn rows_alone_leave_on_codes_and_the_picture_does_not() {
        let scan = harness();
        let shape = scan.shape();
        assert_eq!(shape.outputs.len(), 1);
        assert_eq!(shape.outputs[0].name, "codes");
        assert!(
            !shape.pure,
            "which sighting a code belongs to outlives the frame"
        );
        assert_eq!(shape.find_input("v").unwrap().window, 1);
    }

    #[test]
    fn a_code_in_view_writes_a_row_on_every_frame_named_by_its_first() {
        let rows = run(8, |k| if (2..6).contains(&k) { vec![A] } else { vec![] });
        let frames: Vec<i64> = rows.iter().map(|(pts, _)| *pts).collect();
        assert_eq!(frames, vec![2, 3, 4, 5]);
        assert!(rows.iter().all(|(_, text)| *text == row(2, 0, "ffrwd")));
    }

    #[test]
    fn a_gap_of_fourteen_frames_is_still_the_same_sighting() {
        let rows = run(40, |k| if k == 5 || k == 20 { vec![A] } else { vec![] });
        assert_eq!(
            rows,
            vec![(5, row(5, 0, "ffrwd")), (20, row(5, 0, "ffrwd"))]
        );
    }

    #[test]
    fn a_gap_of_fifteen_frames_starts_a_new_sighting() {
        let rows = run(40, |k| if k == 5 || k == 21 { vec![A] } else { vec![] });
        assert_eq!(
            rows,
            vec![(5, row(5, 0, "ffrwd")), (21, row(21, 1, "ffrwd"))]
        );
    }

    #[test]
    fn two_codes_in_view_are_two_sightings_and_their_rows_are_sorted() {
        let rows = run(6, |k| match k {
            1 => vec![B],
            2..=3 => vec![B, A],
            _ => vec![],
        });
        assert_eq!(
            rows,
            vec![
                (1, row(1, 0, "https://want.video")),
                (2, row(2, 1, "ffrwd")),
                (2, row(1, 0, "https://want.video")),
                (3, row(2, 1, "ffrwd")),
                (3, row(1, 0, "https://want.video")),
            ]
        );
    }

    #[test]
    fn two_codes_first_seen_together_are_told_apart_by_id() {
        let rows = run(3, |k| if k >= 1 { vec![A, B] } else { vec![] });
        assert_eq!(
            rows,
            vec![
                (1, row(1, 0, "ffrwd")),
                (1, row(1, 1, "https://want.video")),
                (2, row(1, 0, "ffrwd")),
                (2, row(1, 1, "https://want.video")),
            ]
        );
    }

    #[test]
    fn the_last_frame_writes_its_rows_like_any_other() {
        let rows = run(3, |k| if k == 2 { vec![A] } else { vec![] });
        assert_eq!(rows, vec![(2, row(2, 0, "ffrwd"))]);
    }

    #[test]
    fn a_last_call_with_no_frame_writes_nothing() {
        let mut scan = harness();
        let emitted = scan.process(&scan.tick(0).last()).unwrap();
        assert!(emitted.messages("codes").is_empty());
    }
}
