-- The clip with every QR code pixelated out of the picture.
-- variables: source (input media path), track (video track index, defaults to the first), dest (output path)
-- example: ffrwd compile -f packages/ffrwd/rsqr/recipes/redact.sql -v source=desk.mp4 -v dest=redacted.mp4
COPY (
  SELECT ffrwd.rsqr.mosaic_codes(v), f.audio
  FROM input(:'source') f, unnest(f.video) v
  WHERE v.index = COALESCE(:track, 1)
) TO :'dest' WITH (video_codec 'libx264', crf 20)
