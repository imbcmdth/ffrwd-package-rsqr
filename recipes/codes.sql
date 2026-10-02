-- The picture and its audio, with a caption track of every QR payload the
-- clip shows: one cue per sighting, merged from the rows scan writes a frame.
-- variables: source (input media path), dest (output path)
-- example: ffrwd compile -f packages/ffrwd/rsqr/recipes/codes.sql -v source=shelf.mp4 -v dest=labelled.mkv
COPY (
  SELECT f.video[1], f.audio, ffrwd.merge_spans(ffrwd.rsqr.scan(f.video[1]), max_span => 30)
  FROM input(:'source') f
) TO :'dest'
