# ffrwd/rsqr

QR codes in video: read them into a caption track, or mosaic them out
of the picture.

Requires ffrwd 0.29.

This is the Rust twin of [`ffrwd/jsqr`](https://github.com/imbcmdth/ffrwd-package-jsqr),
built to the same specification so the two can be timed against each
other: same exports, same rows, same fifteen-frame look-ahead, same
block rule, same pixel format. Only the guest language differs. `scan`'s
rows on a test clip are byte-identical between them.

Use this one. `jsqr` exists to show that JavaScript reaches the world
at all, and [what that costs](#the-comparison).

## Install

```
ffrwd install ffrwd/rsqr
```

## Exports

### `scan(v)` → `STRUCT(start_t number, text text)[]`

A row per frame for each code in view: the decoded payload as `text`,
and as `start_t` the time this sighting of the code began. Every row of
one sighting carries the same `start_t`, which names the sighting, and
each row leaves with the frame it describes.

`ffrwd.merge_spans` turns the rows into one cue per sighting, from the
frame the code was first read to the end of the last frame it was read
on. Selected beside the picture, the cues mint a subtitle track:

```sql
COPY (
  SELECT f.video[1], f.audio,
         ffrwd.merge_spans(ffrwd.rsqr.scan(f.video[1]), max_span => 30)
  FROM input('shelf.mp4') f
) TO 'labelled.mkv'
```

Send `scan`'s rows, or the merged ones, to a `.ndjson` destination
instead and you get the rows themselves, one JSON object per line.

### `mosaic_codes(v)` → `video_stream`

The picture with every code pixelated in place. Mosaic rather than
blur, deliberately: a QR code carries up to 30% error correction and
reads straight through a blur, and deblurring a redacted code is a
known attack. The block is an eighth of each code's own box and never
under two pixels, which is what keeps it larger than the code's
modules; a fixed pixel count would quietly become decorative as
resolution rose.

```sql
COPY (
  SELECT ffrwd.rsqr.mosaic_codes(v), f.audio
  FROM input('desk.mp4') f, unnest(f.video) v
  WHERE v.index = 1
) TO 'redacted.mp4' WITH (video_codec 'libx264', crf 20)
```

## Recipes

- `codes`: the clip with a caption track of its QR payloads.
- `redact`: the clip with its QR codes mosaiced out.

```
ffrwd run ffrwd/rsqr:codes -v source=shelf.mp4 -v dest=labelled.mkv
```

## Flicker

A decoder run frame by frame flickers: the same code reads on one
frame, misses on the next. Each export closes those gaps in the way its
output allows.

`scan` keeps a sighting open while its code goes unread for up to
fourteen frames in a row, so a code the decoder drops for a few frames
is still one sighting and one cue. It does not reach back: each row is
written on the frame it describes, with nothing ahead of that frame
read, so a cue starts on the first frame the code was read.

`mosaic_codes` looks ahead instead. A frame is redacted by every code
found in it or in the fourteen frames after it, so a code is covered
from before the decoder first managed to read it, and a short gap heals
from both sides. The host hands it fifteen frames a call; each is still
decoded only once, since the overlap hits a cache keyed by timestamp,
and a frame with nothing to redact leaves without being copied.

The fifteen frames are the module's own constant, not a parameter.

## When a cue comes out

`ffrwd.merge_spans` writes a sighting's cue once the stream is
`max_span` seconds past its start, or once the stream ends. In the
`codes` recipe that is 30 seconds, so the caption track trails the
picture by up to 30 seconds at the muxer, and a code on screen for
longer than that is written as consecutive cues.

Two codes first read on the same frame come out as one cue for now,
carrying the later payload: the reducer names a sighting by its start
alone. `scan`'s rows hold both.

## From 0.1

0.1's `scan` returned the picture with a cue per run beside it, and a
cue's start reached back fourteen frames before the code was first
read. 0.2 writes the rows and leaves the cues to `ffrwd.merge_spans`,
so on the same clip:

- a cue starts on the frame the code was first read, up to fourteen
  frames later than in 0.1;
- a cue ends at the end of the last frame the code was read on, one
  frame later than in 0.1, which ended at that frame's start;
- the picture does not pass through the module: the recipe copies it
  from the source, and `codes` takes the first video track (it no
  longer takes `track`).

`mosaic_codes`, and so `redact`, are frame for frame what they were.

## The comparison

One clip, one QR code, 60 frames, three builds of the same algorithm,
0.2 on ffrwd 0.29 with 0.1 on ffrwd 0.28 as published in brackets.
Identical rows from every 0.2 run; each time is the fastest of two
runs (three for rsqr), whole pipeline including ffmpeg and process
start:

| | 320×240 | 640×480 | component |
| --- | --- | --- | --- |
| **rsqr** (Rust) | **1.44 s** (0.99 s) | **1.74 s** (1.54 s) | 0.49 MiB (0.34 MiB) |
| jsqr, AOT | 14.06 s (12.70 s) | 40.98 s (37.98 s) | 22.1 MiB (21.0 MiB) |
| jsqr, interpreted | 16.71 s (14.73 s) | 57.54 s (48.12 s) | 13.8 MiB (13.6 MiB) |

Rust is **10× faster at 320×240 and 24× at 640×480**, in a component
forty-five times smaller. The gap widens with frame size because the JS
side pays per pixel in an interpreter while the Rust side runs
compiled, autovectorized code, so the bigger the picture, the worse the
trade.

Most of what rsqr 0.2 adds is the command line, not the module. With
the compile left out, the two pipelines take the same time on this
clip: 0.43 s and 0.72 s for 0.1, 0.44 s and 0.71 s for 0.2. The rest is
the development build of ffrwd 0.29 these runs used, which starts in
0.8 s where 0.28 starts in 0.4 s, and asks each node for its shape
while it compiles. Run again here on the same clip, 0.1 takes 0.95 s and
1.23 s for rsqr, 13.64 s and 41.57 s for jsqr ahead of time, and
17.25 s and 61.10 s interpreted. jsqr 0.2 pays more to start, since
the compiler opens the component to ask it for its shape, and less a
frame, since its `scan` reads one frame a call instead of fifteen.

Two caveats on fairness, both against Rust: this crate builds with
`simd128` (as the other Rust modules do) and ComponentizeJS has no
equivalent, and `rqrr` always attempts the mirrored grid where jsQR is
told not to. Neither is worth a quarter of the difference.

## The decoder

[`rqrr`](https://github.com/WanzenBug/rqrr) 0.10, a pure-Rust port of
quirc, with its `img` feature off: that feature exists to load files,
which a module handed raw pixels never does.

`rqrr` reports its box as the decoded grid's hull, where jsQR reports
the finder-pattern hull. The rectangles are close but not equal, so
`mosaic_codes` is not pixel-identical to `jsqr`'s even though `scan` is
byte-identical. Multi-code frames are handled by painting each found
code out and rescanning, capped at eight passes, and a frame's rows are
written in the order of their payloads, so the two decoders agree on
it.

## Building it

```
cargo build --release --target wasm32-wasip2
cargo test
```

Both modules are nodes written with
[`ffrwd-node`](https://github.com/imbcmdth/ffrwd-node), which carries
the world they are built against, so there is no `build.rs`, no wit in
this tree and nothing to install first. The tests run on the host and
need no wasm and no ffmpeg: they generate QR codes in memory, paint
them to rgba, and run the detection core and both nodes over the
pixels.

## License

**Apache-2.0**. `rqrr` is MIT OR Apache-2.0 AND ISC.
