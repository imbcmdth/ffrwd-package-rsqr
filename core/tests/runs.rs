// The retroactive reach, driven the way the host drives it: no pixels, no
// wasm. `heads` says which frames a call speaks for; `CodeRuns` turns the
// credit each of them gets into cues.

use rsqr_core::{heads, CodeRuns, Cue, Sightings, SCAN_WINDOW, WINDOW};

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

/// One call: the frames it was handed, and whether it is the final one.
struct Call {
    frames: Vec<usize>,
    last: bool,
}

/// The calls a `window`-wide, stride-1 cut makes over `count` frames, the way
/// the host cuts them: a call as soon as the window fills, then a final call
/// over whatever the last stride left over.
fn calls(count: usize, window: usize) -> Vec<Call> {
    let mut cut = Vec::new();
    let mut buffered: Vec<usize> = Vec::new();
    for frame in 0..count {
        buffered.push(frame);
        if buffered.len() == window {
            cut.push(Call {
                frames: buffered.clone(),
                last: false,
            });
            buffered.remove(0);
        }
    }
    cut.push(Call {
        frames: buffered,
        last: true,
    });
    cut
}

/// Which frame each call of that cut speaks for, in the order they are spoken
/// for.
fn spoken_for(count: usize, window: usize) -> Vec<usize> {
    calls(count, window)
        .iter()
        .flat_map(|call| heads(call.frames.len(), call.last).map(|head| call.frames[head]))
        .collect()
}

/// What the whole run produced: the cues, and the frame the call that emitted
/// each one spoke for - `None` for the ones the final flush produced.
struct Driven {
    cues: Vec<Cue>,
    emitted: Vec<Option<usize>>,
}

/// The whole stream through `scan`: one frame read per call, in the order the
/// host hands them over, then the flush that closes whatever is still open.
/// `seen(k)` is the payloads frame k was really decoded in.
fn drive(count: usize, seen: impl Fn(usize) -> Vec<&'static str>) -> Driven {
    let mut runs = CodeRuns::new();
    let mut driven = Driven {
        cues: Vec::new(),
        emitted: Vec::new(),
    };
    for frame in spoken_for(count, SCAN_WINDOW as usize) {
        let mut sightings = Sightings::new();
        for text in seen(frame) {
            sightings.note(text, at(frame));
        }
        for one in runs.credit(at(frame), &sightings) {
            driven.cues.push(one);
            driven.emitted.push(Some(frame));
        }
    }
    for one in runs.flush() {
        driven.cues.push(one);
        driven.emitted.push(None);
    }
    driven
}

#[test]
fn every_frame_is_spoken_for_exactly_once_in_order() {
    let wanted: Vec<usize> = (0..40).collect();
    assert_eq!(spoken_for(40, WINDOW as usize), wanted);
    assert_eq!(spoken_for(40, SCAN_WINDOW as usize), wanted);
}

#[test]
fn a_stream_shorter_than_the_window_is_all_one_final_call() {
    let calls = calls(4, WINDOW as usize);
    assert_eq!(calls.len(), 1);
    assert!(calls[0].last);
    assert_eq!(spoken_for(4, WINDOW as usize), vec![0, 1, 2, 3]);
}

#[test]
fn the_scan_window_leaves_the_last_frame_for_the_final_call() {
    // A window of one would come out even, and the closing cues would have no
    // frame left to ride out on.
    let cut = calls(40, SCAN_WINDOW as usize);
    let last = cut.last().expect("a final call is always made");
    assert!(last.last);
    assert_eq!(last.frames, vec![39]);
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
fn a_run_is_only_known_to_have_ended_a_window_after_its_last_sighting() {
    // Nothing ahead of the frame in hand is read, so a run that ends at frame
    // 13 is only certainly over once frame 12 + WINDOW has gone by unseen.
    let driven = drive(40, |k| if k == 12 { vec!["a"] } else { vec![] });
    assert_eq!(driven.emitted, vec![Some(12 + WINDOW as usize)]);
}

#[test]
fn a_run_that_outlives_the_stream_is_closed_by_the_flush() {
    let driven = drive(40, |k| if k >= 30 { vec!["a"] } else { vec![] });
    assert_eq!(driven.emitted, vec![None]);
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
fn cues_the_stream_ends_on_come_out_in_the_order_their_runs_ended() {
    // "late" is on screen to the end; "early" leaves a few frames before it,
    // too few for the window to finish missing. Both are closed by the same
    // flush, and the one that ended first comes out first however they were
    // met.
    let driven = drive(40, |k| match k {
        21..=34 => vec!["late", "early"],
        20..=38 => vec!["late"],
        _ => vec![],
    });
    let order: Vec<&str> = driven.cues.iter().map(|one| one.text.as_str()).collect();
    assert_eq!(order, vec!["early", "late"]);
    assert_eq!(driven.emitted, vec![None, None]);
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
