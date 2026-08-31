-- The picture and its audio, with a caption track of every QR payload the
-- clip shows.
-- variables: source (input media path), track (video track index, defaults to the first), dest (output path)
-- example: ffrwd compile -f packages/ffrwd/rsqr/recipes/codes.sql -v source=shelf.mp4 -v dest=labelled.mkv
COPY (
  SELECT v, f.audio, ffrwd.rsqr.scan(v).codes
  FROM input(:'source') f, unnest(f.video) v
  WHERE v.index = COALESCE(:track, 1)
) TO :'dest'
