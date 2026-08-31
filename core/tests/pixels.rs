// The pixel work over the rgba frames the wire carries: the luma the decoder
// reads, the box around a code, and the mosaic that redacts it.

use rsqr_core::{block_size, bounding_box, greyscale, mosaic_box, BoxRect, CHANNELS};

fn pixel(r: u8, g: u8, b: u8) -> Vec<u8> {
    vec![r, g, b, 255]
}

#[test]
fn luma_weights_green_most_and_blue_least() {
    assert_eq!(greyscale(&pixel(255, 0, 0), 1, 0, 0), 54);
    assert_eq!(greyscale(&pixel(0, 255, 0), 1, 0, 0), 182);
    assert_eq!(greyscale(&pixel(0, 0, 255), 1, 0, 0), 18);
}

#[test]
fn grey_stays_itself_and_the_ends_reach_the_ends() {
    assert_eq!(greyscale(&pixel(0, 0, 0), 1, 0, 0), 0);
    assert_eq!(greyscale(&pixel(128, 128, 128), 1, 0, 0), 128);
    assert_eq!(greyscale(&pixel(255, 255, 255), 1, 0, 0), 255);
}

#[test]
fn alpha_takes_no_part_in_the_luma() {
    let opaque = [10u8, 20, 30, 255];
    let clear = [10u8, 20, 30, 0];
    assert_eq!(greyscale(&opaque, 1, 0, 0), greyscale(&clear, 1, 0, 0));
}

#[test]
fn the_luma_is_read_at_the_pixel_asked_for() {
    // Two pixels on one row: black, then white.
    let row = [0u8, 0, 0, 255, 255, 255, 255, 255];
    assert_eq!(greyscale(&row, 2, 0, 0), 0);
    assert_eq!(greyscale(&row, 2, 1, 0), 255);
}

#[test]
fn a_64_pixel_box_mosaics_in_8_pixel_blocks() {
    assert_eq!(block_size(64), 8);
}

#[test]
fn a_small_box_floors_at_two_pixels() {
    assert_eq!(block_size(9), 2);
    assert_eq!(block_size(1), 2);
    assert_eq!(block_size(0), 2);
}

#[test]
fn the_block_grows_with_the_box_so_it_always_swallows_a_module() {
    assert_eq!(block_size(160), 20);
    assert_eq!(block_size(800), 100);
}

#[test]
fn the_box_is_the_axis_aligned_hull_of_the_four_corners_clipped() {
    let corners = [(10.4, 20.2), (70.9, 18.5), (72.1, 80.6), (8.7, 82.3)];
    assert_eq!(
        bounding_box(corners, 200, 200),
        BoxRect {
            x: 8,
            y: 18,
            w: 65,
            h: 65
        }
    );
}

#[test]
fn a_box_running_off_the_frame_is_cut_to_it() {
    let corners = [(-5.0, -5.0), (300.0, -5.0), (300.0, 300.0), (-5.0, 300.0)];
    assert_eq!(
        bounding_box(corners, 100, 100),
        BoxRect {
            x: 0,
            y: 0,
            w: 100,
            h: 100
        }
    );
}

#[test]
fn a_box_entirely_past_the_frame_still_has_a_pixel_of_width() {
    let corners = [
        (300.0, 300.0),
        (400.0, 300.0),
        (400.0, 400.0),
        (300.0, 400.0),
    ];
    let bbox = bounding_box(corners, 100, 100);
    assert_eq!(bbox.w, 1);
    assert_eq!(bbox.h, 1);
}

#[test]
fn a_mosaiced_block_is_its_own_average_and_nothing_outside_it_moves() {
    let width = 4;
    let height = 2;
    let mut rgba = vec![0u8; width * height * CHANNELS];
    let mut set = |x: usize, y: usize, value: u8| {
        let i = (y * width + x) * CHANNELS;
        rgba[i] = value;
        rgba[i + 1] = value;
        rgba[i + 2] = value;
        rgba[i + 3] = 255;
    };
    set(0, 0, 0);
    set(1, 0, 200);
    set(0, 1, 0);
    set(1, 1, 200);
    set(3, 0, 111);

    mosaic_box(
        &mut rgba,
        width,
        height,
        &BoxRect {
            x: 0,
            y: 0,
            w: 2,
            h: 2,
        },
    );

    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let i = (y * width + x) * CHANNELS;
        assert_eq!(rgba[i], 100);
        assert_eq!(rgba[i + 3], 255, "alpha is left alone");
    }
    assert_eq!(rgba[3 * CHANNELS], 111);
}

#[test]
fn the_mosaic_leaves_no_run_of_the_original_pixels_behind() {
    let width = 32;
    let height = 32;
    let mut rgba = vec![0u8; width * height * CHANNELS];
    // A checkerboard one pixel wide: nothing survives a block larger than one.
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * CHANNELS;
            let value = if (x + y) % 2 == 0 { 0 } else { 255 };
            rgba[i] = value;
            rgba[i + 1] = value;
            rgba[i + 2] = value;
            rgba[i + 3] = 255;
        }
    }
    mosaic_box(
        &mut rgba,
        width,
        height,
        &BoxRect {
            x: 0,
            y: 0,
            w: 32,
            h: 32,
        },
    );
    let mut colours: Vec<u8> = rgba.iter().step_by(CHANNELS).copied().collect();
    colours.dedup();
    assert_eq!(colours, vec![128]);
}
