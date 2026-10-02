// The row `scan` writes for a code in view, which `ffrwd/jsqr` writes byte for
// byte the same, and the order a frame's codes are written in.

use rsqr_core::{payloads, BoxRect, Found, Sighting};

fn sighting(start_t: f64, text: &str) -> String {
    Sighting {
        start_t,
        id: 3,
        text: text.to_string(),
    }
    .row()
}

fn found(texts: &[&str]) -> Vec<Found> {
    texts
        .iter()
        .map(|text| Found {
            text: text.to_string(),
            bbox: BoxRect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
        })
        .collect()
}

#[test]
fn a_row_writes_a_whole_second_without_a_fraction() {
    assert_eq!(
        sighting(1.0, "ffrwd"),
        r#"{"start_t":1,"id":3,"text":"ffrwd"}"#
    );
    assert_eq!(
        sighting(0.0, "ffrwd"),
        r#"{"start_t":0,"id":3,"text":"ffrwd"}"#
    );
}

#[test]
fn a_row_keeps_every_digit_of_a_time_that_needs_them() {
    assert_eq!(
        sighting(1.9666666666666663, "a"),
        r#"{"start_t":1.9666666666666663,"id":3,"text":"a"}"#
    );
}

#[test]
fn a_payload_with_json_in_it_is_escaped_into_the_row() {
    assert_eq!(
        sighting(0.0, "he said \"hi\"\n"),
        r#"{"start_t":0,"id":3,"text":"he said \"hi\"\n"}"#
    );
}

#[test]
fn a_frame_names_each_payload_once_in_sorted_order() {
    let codes = found(&["right", "left", "right"]);
    assert_eq!(payloads(&codes), vec!["left", "right"]);
}

#[test]
fn payloads_sort_as_javascript_sorts_them() {
    // U+FF5E is above every surrogate in UTF-16 and below U+1F600 in UTF-8.
    let codes = found(&["\u{FF5E}", "\u{1F600}"]);
    assert_eq!(payloads(&codes), vec!["\u{1F600}", "\u{FF5E}"]);
}

#[test]
fn a_frame_with_no_code_names_nothing() {
    assert!(payloads(&[]).is_empty());
}
