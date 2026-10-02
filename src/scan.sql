-- The reading export, hosted as the wasm module the package ships.
--
-- `scan` returns a row per frame for each QR code in view: the decoded
-- payload as `text`, as `start_t` the time this sighting of the code began,
-- and as `id` how many sightings began before it. Every row of one sighting
-- carries the same `start_t` and `id`. A code the decoder misses for up to 14
-- frames in a row is still the same sighting, so the flicker a per-frame
-- decoder produces stays inside it. `ffrwd.merge_spans` turns the rows into
-- one cue per sighting.
CREATE FUNCTION scan(v video_stream)
RETURNS STRUCT(start_t number, id number, text text)[]
  AS 'target/wasm32-wasip2/release/scan.wasm', 'scan' LANGUAGE wasm;
