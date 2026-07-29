CREATE TABLE IF NOT EXISTS asset_captions (
  asset_id TEXT PRIMARY KEY NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  caption TEXT NOT NULL,
  model_id TEXT NOT NULL,
  created_at TEXT NOT NULL
);

DROP TABLE IF EXISTS assets_fts;

CREATE VIRTUAL TABLE assets_fts USING fts5(
  asset_id UNINDEXED,
  filename,
  tags,
  camera,
  lens,
  ocr_text,
  people,
  auto_tags,
  caption,
  tokenize='porter'
);
