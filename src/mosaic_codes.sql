-- The redacting export, over the same detection core and the same window.
--
-- `mosaic_codes` returns the picture with every QR code pixelated in place,
-- a frame redacted by the codes found in the 15 frames from it on - so a code
-- is covered from before the decoder first read it. The block is an eighth of
-- each code's own box, never under 2 pixels, which is what puts it over the
-- code's module size: a QR code carries up to 30% error correction and reads
-- straight through a blur, so a mosaic that does not swallow whole modules
-- only looks like a redaction.
CREATE FUNCTION mosaic_codes(v video_stream)
RETURNS video_stream
  AS 'target/wasm32-wasip2/release/mosaic_codes.wasm', 'mosaic_codes' LANGUAGE wasm;
