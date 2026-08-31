-- The reading export, hosted as the wasm module the package ships.
--
-- `scan` returns the picture untouched with one cue per QR code beside it:
-- the decoded payload as the cue's text, and the span the code was on screen
-- for. The module reads 15 frames at a time and credits the first of them
-- with every code found anywhere in the window, so the flicker a per-frame
-- decoder produces closes up. That window is the module's own constant - the
-- host settles it before the call is opened - so there is nothing here to
-- pass.
CREATE FUNCTION scan(v video_stream)
RETURNS STRUCT(v video_stream, codes cue[])
  AS 'target/wasm32-wasip2/release/scan.wasm', 'scan' LANGUAGE wasm;
