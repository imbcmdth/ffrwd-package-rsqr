// The retroactive window, driven the way the host drives it: no pixels, no
// wasm. `heads` says which frames a call speaks for; `CodeRuns` turns the
// credit each of them gets into cues.

use rsqr_core::{heads, CodeRuns, Cue, Sightings, WINDOW};

const FPS: f64 = 30.0;

fn at(index: usize) -> f64 {
    index as f64 / FPS
}

fn cue(text: &str, start: usize, end: usize) -> Cue {
    Cue {
        text: text.to_string(),
        start_t: at(start),
        end_t: at(end),
    }
}

/// One frame of a driven stream: its index, and the payloads it was really
/// decoded in.
struct Frame {
    pts: usize,
    texts: Vec<&'static str>,
}

/// One call: the window it was handed, and whether it is the final one.
struct Call {
    from: usize,
    to: usize,
    last: bool,
}

/// What the whole run produced: the cues, and which frame was credited with
/// what.
struct Driven {
    cues: Vec<Cue>,
    credited: Vec<(usize, Vec<String>)>,
}

/// The whole stream through the module, the way the sidecar cuts it: a call
/// per frame carrying that frame and the WINDOW - 1 after it, then a final
/// call carrying whatever the last stride left over. `seen(k)` is the payloads
/// frame k was really decoded in.
fn drive(count: usize, seen: impl Fn(usize) -> Vec<&'static str>) -> Driven {
    let frames: Vec<Frame> = (0..count)
        .map(|pts| Frame {
            pts,
            texts: seen(pts),
        })
        .collect();
    let size = WINDOW as usize;

    let mut calls: Vec<Call> = Vec::new();
    let mut consumed = 0;
    while consumed + size <= count {
        calls.push(Call {
            from: consumed,
            to: consumed + size,
            last: false,
        });
        consumed += 1;
    }
    calls.push(Call {
        from: consumed,
        to: count,
        last: true,
    });

    let mut runs = CodeRuns::new();
    let mut driven = Driven {
        cues: Vec::new(),
        credited: Vec::new(),
    };
    for call in calls {
        let window = &frames[call.from..call.to];
        for head in heads(window.len(), call.last) {
            let mut sightings = Sightings::new();
            for frame in &window[head..] {
                for text in &frame.texts {
                    sightings.note(text, at(frame.pts));
                }
            }
            driven.credited.push((
                window[head].pts,
                sightings.iter().map(|(text, _)| text.to_string()).collect(),
            ));
            driven
                .cues
                .extend(runs.credit(at(window[head].pts), &sightings));
        }
    }
    driven.cues.extend(runs.flush());
    driven
}

#[test]
fn every_frame_is_spoken_for_exactly_once_in_order() {
    let driven = drive(40, |_| vec![]);
    let spoken: Vec<usize> = driven.credited.iter().map(|(frame, _)| *frame).collect();
    assert_eq!(spoken, (0..40).collect::<Vec<usize>>());
}

#[test]
fn a_stream_shorter_than_the_window_is_all_one_final_call() {
    let driven = drive(4, |_| vec![]);
    let spoken: Vec<usize> = driven.credited.iter().map(|(frame, _)| *frame).collect();
    assert_eq!(spoken, vec![0, 1, 2, 3]);
}

#[test]
fn a_call_speaks_for_its_first_frame_and_sees_the_window_from_it() {
    let frames = 3;
    let regular: Vec<(usize, usize)> = heads(frames, false)
        .map(|head| (head, frames - head))
        .collect();
    assert_eq!(regular, vec![(0, 3)]);
}

#[test]
fn the_final_call_speaks_for_every_frame_left_over_over_shortening_windows() {
    let frames = 3;
    let final_call: Vec<(usize, usize)> = heads(frames, true)
        .map(|head| (head, frames - head))
        .collect();
    assert_eq!(final_call, vec![(0, 3), (1, 2), (2, 1)]);
}

#[test]
fn a_sighting_is_carried_back_over_every_frame_of_its_window() {
    let driven = drive(40, |k| if k == 12 { vec!["a"] } else { vec![] });
    // Frame 12 is the last frame of the window headed by frame 0 through the
    // window headed by frame 12, so all of them are credited with it.
    let carrying: Vec<usize> = driven
        .credited
        .iter()
        .filter(|(_, texts)| !texts.is_empty())
        .map(|(frame, _)| *frame)
        .collect();
    assert_eq!(carrying, (0..=12).collect::<Vec<usize>>());
}

#[test]
fn a_code_seen_once_makes_one_cue_from_its_window_back_to_its_sighting() {
    let driven = drive(40, |k| if k == 12 { vec!["a"] } else { vec![] });
    assert_eq!(driven.cues, vec![cue("a", 0, 12)]);
}

#[test]
fn a_code_first_seen_well_into_the_stream_starts_a_window_earlier() {
    let driven = drive(60, |k| if k == 30 { vec!["a"] } else { vec![] });
    assert_eq!(driven.cues, vec![cue("a", 30 - WINDOW as usize + 1, 30)]);
}

#[test]
fn a_gap_the_window_can_span_heals_into_one_run() {
    let seen = [5, 6, 8, 9];
    let driven = drive(40, move |k| {
        if seen.contains(&k) {
            vec!["ffrwd"]
        } else {
            vec![]
        }
    });
    assert_eq!(driven.cues, vec![cue("ffrwd", 0, 9)]);
}

#[test]
fn a_gap_the_window_cannot_span_breaks_the_run_in_two() {
    let seen = [20, 21, 60, 61];
    let driven = drive(90, move |k| {
        if seen.contains(&k) {
            vec!["ffrwd"]
        } else {
            vec![]
        }
    });
    let window = WINDOW as usize;
    assert_eq!(
        driven.cues,
        vec![
            cue("ffrwd", 20 - window + 1, 21),
            cue("ffrwd", 60 - window + 1, 61),
        ]
    );
}

#[test]
fn one_cue_covers_a_whole_run_however_many_frames_it_was_seen_in() {
    let driven = drive(90, |k| {
        if (30..=60).contains(&k) {
            vec!["a"]
        } else {
            vec![]
        }
    });
    assert_eq!(driven.cues, vec![cue("a", 30 - WINDOW as usize + 1, 60)]);
}

#[test]
fn two_codes_on_screen_together_make_one_cue_each() {
    let driven = drive(60, |k| {
        if (20..=30).contains(&k) {
            vec!["left", "right"]
        } else {
            vec![]
        }
    });
    let mut texts: Vec<&str> = driven.cues.iter().map(|cue| cue.text.as_str()).collect();
    texts.sort_unstable();
    assert_eq!(texts, vec!["left", "right"]);
    for one in &driven.cues {
        assert_eq!(one.start_t, at(20 - WINDOW as usize + 1));
        assert_eq!(one.end_t, at(30));
    }
}

#[test]
fn a_code_still_on_screen_when_the_stream_ends_is_closed_by_the_flush() {
    let count = 40;
    let driven = drive(count, |k| if k >= 20 { vec!["a"] } else { vec![] });
    assert_eq!(
        driven.cues,
        vec![cue("a", 20 - WINDOW as usize + 1, count - 1)]
    );
}

#[test]
fn a_code_caught_in_one_frame_alone_still_spans_a_frame() {
    let mut runs = CodeRuns::new();
    let mut sightings = Sightings::new();
    sightings.note("a", 0.0);
    assert!(runs.credit(0.0, &sightings).is_empty());
    let cues = runs.flush();
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].start_t, 0.0);
    assert!(cues[0].end_t > cues[0].start_t);
    assert!(runs.flush().is_empty());
}

#[test]
fn a_frame_with_no_code_credits_nothing_and_closes_nothing() {
    let mut runs = CodeRuns::new();
    assert!(runs.credit(0.0, &Sightings::new()).is_empty());
    assert!(runs.flush().is_empty());
}

#[test]
fn a_cue_writes_a_whole_second_without_a_fraction() {
    let row = cue("ffrwd", 0, 30).row();
    assert_eq!(row, r#"{"text":"ffrwd","start_t":0,"end_t":1}"#);
}

#[test]
fn a_cue_keeps_every_digit_of_a_time_that_needs_them() {
    let row = Cue {
        text: "a".to_string(),
        start_t: 0.0,
        end_t: 1.9666666666666663,
    }
    .row();
    assert_eq!(
        row,
        r#"{"text":"a","start_t":0,"end_t":1.9666666666666663}"#
    );
}

#[test]
fn a_payload_with_json_in_it_is_escaped_into_the_row() {
    let row = Cue {
        text: "he said \"hi\"".to_string(),
        start_t: 0.0,
        end_t: 1.0,
    }
    .row();
    assert_eq!(row, r#"{"text":"he said \"hi\"","start_t":0,"end_t":1}"#);
}
