-- The reading export, hosted as the wasm module the package ships.
--
-- `scan` returns the picture untouched with one cue per QR code beside it:
-- the decoded payload as the cue's text, and the span the code was on screen
-- for. The module reads one frame at a time and credits a code back over the
-- 15 frames before the one it was read in, out of the timestamps it kept as
-- they passed, so the flicker a per-frame decoder produces closes up without
-- the pixels of those frames being asked for twice. That reach is the
-- module's own constant, so there is nothing here to pass.
CREATE FUNCTION scan(v video_stream)
RETURNS STRUCT(v video_stream, codes cue[])
  AS 'target/wasm32-wasip2/release/scan.wasm', 'scan' LANGUAGE wasm;
