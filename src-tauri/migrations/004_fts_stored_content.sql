-- contentless FTS5 (`content=''`) does not store UNINDEXED columns, so
-- JOIN assets_fts ON asset_id always failed. Rebuild with stored columns.
-- Row content is repopulated from Rust after this migration applies.
DROP TABLE IF EXISTS assets_fts;

CREATE VIRTUAL TABLE assets_fts USING fts5(
  asset_id UNINDEXED,
  filename,
  tags,
  camera,
  lens,
  tokenize='porter'
);
