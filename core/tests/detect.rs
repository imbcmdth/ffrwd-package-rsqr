// Detection against codes made here: a QR encoder writes the matrix, this
// file paints it into rgba pixels, and the module's own core reads it back.
// No binary fixture, and the round trip is what proves the decoder runs.

use qrcode::{Color, EcLevel, QrCode};
use rsqr_core::{detect_codes, mosaic_box, DetectionCache, Instance, WindowFrame, CHANNELS};

const SCALE: usize = 6;
const QUIET: usize = 4;

struct Rendered {
    rgba: Vec<u8>,
    width: usize,
    height: usize,
}

fn white(width: usize, height: usize) -> Vec<u8> {
    vec![255u8; width * height * CHANNELS]
}

// One code's matrix as a square of rgba pixels, dark modules on white, with
// the quiet zone a decoder needs around it.
fn render(text: &str, scale: usize) -> Rendered {
    let code = QrCode::with_error_correction_level(text, EcLevel::M).expect("a code for the text");
    let modules = code.width();
    let colors = code.to_colors();
    let side = (modules + QUIET * 2) * scale;
    let mut rgba = white(side, side);
    for row in 0..modules {
        for column in 0..modules {
            if colors[row * modules + column] != Color::Dark {
                continue;
            }
            let x0 = (column + QUIET) * scale;
            let y0 = (row + QUIET) * scale;
            for y in y0..y0 + scale {
                for x in x0..x0 + scale {
                    let i = (y * side + x) * CHANNELS;
                    rgba[i] = 0;
                    rgba[i + 1] = 0;
                    rgba[i + 2] = 0;
                }
            }
        }
    }
    Rendered {
        rgba,
        width: side,
        height: side,
    }
}

// Pastes one rendered code into a larger frame at (x, y).
fn paste(frame: &mut [u8], width: usize, code: &Rendered, x: usize, y: usize) {
    for row in 0..code.height {
        let from = row * code.width * CHANNELS;
        let to = ((y + row) * width + x) * CHANNELS;
        let span = code.width * CHANNELS;
        frame[to..to + span].copy_from_slice(&code.rgba[from..from + span]);
    }
}

#[test]
fn a_rendered_code_decodes_back_to_its_own_payload() {
    let code = render("ffrwd", SCALE);
    let found = detect_codes(&code.rgba, code.width, code.height);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "ffrwd");
}

#[test]
fn the_frame_handed_to_the_decoder_is_not_written_to() {
    let code = render("ffrwd", SCALE);
    let before = code.rgba.clone();
    detect_codes(&code.rgba, code.width, code.height);
    assert_eq!(code.rgba, before);
}

#[test]
fn the_box_found_around_a_code_covers_the_code_and_not_the_quiet_zone() {
    let code = render("ffrwd", SCALE);
    let found = detect_codes(&code.rgba, code.width, code.height);
    let bbox = found[0].bbox;
    let inset = QUIET * SCALE;
    assert!(bbox.x + SCALE >= inset, "box.x {}", bbox.x);
    assert!(bbox.y + SCALE >= inset, "box.y {}", bbox.y);
    assert!(
        bbox.x + bbox.w <= code.width - inset + SCALE,
        "box right {}",
        bbox.x + bbox.w
    );
    assert!(bbox.h > SCALE * 8, "box.h {}", bbox.h);
}

#[test]
fn a_longer_payload_survives_the_round_trip() {
    let text = "https://want.video/ffrwd/rsqr";
    let code = render(text, SCALE);
    let found = detect_codes(&code.rgba, code.width, code.height);
    let texts: Vec<&str> = found.iter().map(|one| one.text.as_str()).collect();
    assert_eq!(texts, vec![text]);
}

#[test]
fn two_codes_in_one_frame_are_both_found() {
    let near = render("near", 10);
    let far = render("far", 4);
    let width = 1000;
    let height = 600;
    let mut frame = white(width, height);
    paste(&mut frame, width, &near, 20, 20);
    paste(&mut frame, width, &far, 700, 400);

    let found = detect_codes(&frame, width, height);
    let mut texts: Vec<&str> = found.iter().map(|one| one.text.as_str()).collect();
    texts.sort_unstable();
    assert_eq!(texts, vec!["far", "near"]);
}

#[test]
fn a_frame_with_no_code_finds_nothing() {
    assert!(detect_codes(&white(120, 120), 120, 120).is_empty());
}

#[test]
fn the_mosaic_over_a_found_box_leaves_the_code_unreadable() {
    let mut code = render("ffrwd", SCALE);
    let bbox = detect_codes(&code.rgba, code.width, code.height)[0].bbox;
    mosaic_box(&mut code.rgba, code.width, code.height, &bbox);
    assert!(detect_codes(&code.rgba, code.width, code.height).is_empty());
}

#[test]
fn a_frame_is_decoded_once_per_timestamp_however_often_it_is_handed_in() {
    let mut cache = DetectionCache::default();
    let code = render("ffrwd", SCALE);
    let first = cache
        .codes_for(7, &code.rgba, code.width, code.height)
        .to_vec();
    let again = cache.codes_for(7, &code.rgba, code.width, code.height);
    assert_eq!(first, again);
    // The same answer, and the second ask did no work.
    assert_eq!(cache.decoded(), 1);
    let texts: Vec<&str> = first.iter().map(|one| one.text.as_str()).collect();
    assert_eq!(texts, vec!["ffrwd"]);
}

#[test]
fn a_window_read_fetches_only_the_timestamps_not_yet_decoded() {
    let code = render("ffrwd", SCALE);
    let mut instance = Instance::new("test");
    instance
        .open(code.width as u32, code.height as u32, "rgba", (1, 30), "")
        .expect("opens");
    // The first call meets pts 0 by the eager path.
    instance.read(&[WindowFrame {
        pts: 0,
        frame: &code.rgba,
    }]);
    // The next window holds pts 0 and 1; only pts 1 is fetched.
    let mut fetched: Vec<usize> = Vec::new();
    let seen = instance.read_fetching(&[0, 1], |i| {
        fetched.push(i);
        code.rgba.clone()
    });
    assert_eq!(fetched, vec![1]);
    // Both frames still count: the cached one and the fetched one agree.
    assert_eq!(seen.boxes.len(), 1);
    assert_eq!(seen.sightings.get("ffrwd"), Some(1.0 / 30.0));
}

#[test]
fn a_timestamp_that_has_left_the_window_is_dropped() {
    let mut cache = DetectionCache::new(2);
    let blank = white(60, 60);
    cache.codes_for(1, &blank, 60, 60);
    cache.codes_for(2, &blank, 60, 60);
    cache.codes_for(3, &blank, 60, 60);
    assert_eq!(cache.decoded(), 3);
    cache.codes_for(1, &blank, 60, 60);
    assert_eq!(cache.decoded(), 4, "the dropped timestamp is decoded again");
}
