# ffrwd/rsqr

QR codes in video: read them into a caption track, or mosaic them out
of the picture.

This is the Rust twin of [`ffrwd/jsqr`](https://github.com/imbcmdth/ffrwd-package-jsqr),
built to the same specification so the two can be timed against each
other — same exports, same fifteen-frame retroactive window, same cue
semantics, same block rule, same pixel format. Only the guest language
differs. `scan`'s output on a test clip is byte-identical between them.

Use this one. `jsqr` exists to show that JavaScript reaches the world
at all, and [what that costs](#the-comparison).

## Install

```
ffrwd install ffrwd/rsqr
```

## Exports

### `scan(v)` → `STRUCT(v video_stream, codes cue[])`

The picture untouched, and one cue per code beside it: the decoded
payload as the cue's text, spanning the time the code was on screen.
Project the `codes` column into a stream position and it mints a
subtitle track:

```sql
COPY (
  SELECT v, f.audio, ffrwd.rsqr.scan(v).codes
  FROM input('shelf.mp4') f, unnest(f.video) v
  WHERE v.index = 1
) TO 'labelled.mkv'
```

Send the same column to a `.ndjson` destination instead and you get the
rows themselves, one JSON object per cue.

### `mosaic_codes(v)` → `video_stream`

The picture with every code pixelated in place. Mosaic rather than
blur, deliberately: a QR code carries up to 30% error correction and
reads straight through a blur, and deblurring a redacted code is a
known attack. The block is an eighth of each code's own box and never
under two pixels, which is what keeps it larger than the code's
modules — a fixed pixel count would quietly become decorative as
resolution rose.

```sql
COPY (
  SELECT ffrwd.rsqr.mosaic_codes(v), f.audio
  FROM input('desk.mp4') f, unnest(f.video) v
  WHERE v.index = 1
) TO 'redacted.mp4' WITH (video_codec 'libx264', crf 20)
```

## Recipes

- `codes` — the clip with a caption track of its QR payloads.
- `redact` — the clip with its QR codes mosaiced out.

```
ffrwd ffrwd.rsqr.codes -v source=shelf.mp4 -v dest=labelled.mkv
```

## Detection is retroactive

A decoder run frame by frame flickers: the same code reads on one
frame, misses on the next, and the output stutters. So the module
reads fifteen frames at a time and credits the first of them with
every code found anywhere in that window. A code the decoder only
catches on the fourteenth frame is carried back over the thirteen
before it, and a gap the window can span heals from both sides.

This costs nothing in latency. The frame the module speaks for is the
oldest one it holds, not the newest, so the look-ahead is the host's
buffer rather than a delay in the output. Each frame is still decoded
exactly once — the fifteen-fold overlap hits a cache keyed by
timestamp.

For `scan` it means a cue starts a window before the code was first
read and ends at its last real sighting: one cue per code per
appearance, not one row per frame. For `mosaic_codes` it means a code
is covered from before the decoder first managed to read it.

The window is fixed at fifteen frames. It is not a parameter, and
cannot be: the host settles a module's window from `describe()` before
it opens the call, so a parameter could only ever narrow behavior
inside a window already sized.

## The comparison

One clip, one QR code, 60 frames, three builds of the same algorithm.
Identical output from all six runs; the fastest of two runs each,
whole pipeline including ffmpeg and process start:

| | 320×240 | 640×480 | component |
| --- | --- | --- | --- |
| **rsqr** (Rust) | **0.99 s** | **1.54 s** | 0.34 MiB |
| jsqr, AOT | 12.70 s | 37.98 s | 21.0 MiB |
| jsqr, interpreted | 14.73 s | 48.12 s | 13.6 MiB |

Rust is **13× faster at 320×240 and 25× at 640×480**, in a component
sixty times smaller. The gap widens with frame size because the JS
side pays per pixel in an interpreter while the Rust side runs
compiled, autovectorized code — so the bigger the picture, the worse
the trade.

Two caveats on fairness, both against Rust: this crate builds with
`simd128` (as the other Rust modules do) and ComponentizeJS has no
equivalent, and `rqrr` always attempts the mirrored grid where jsQR is
told not to. Neither is worth a quarter of the difference.

## The decoder

[`rqrr`](https://github.com/WanzenBug/rqrr) 0.10, a pure-Rust port of
quirc, with its `img` feature off — that feature exists to load files,
which a module handed raw pixels never does.

`rqrr` reports its box as the decoded grid's hull, where jsQR reports
the finder-pattern hull. The rectangles are close but not equal, so
`mosaic_codes` is not pixel-identical to `jsqr`'s even though `scan` is
byte-identical. Multi-code frames are handled by painting each found
code out and rescanning, capped at eight passes.

## Building it

```
ffrwd install -g ffrwd/wasm
cargo build --release --target wasm32-wasip2
cargo test
```

`build.rs` finds the wit by asking `ffrwd path ffrwd/wasm`, so the
world comes from the installed package rather than a copy in this
tree. The tests run on the host and need no wasm and no ffmpeg: they
generate QR codes in memory, paint them to rgba, and run the detection
core over the pixels.

## License

**Apache-2.0**. `rqrr` is MIT OR Apache-2.0 AND ISC.
