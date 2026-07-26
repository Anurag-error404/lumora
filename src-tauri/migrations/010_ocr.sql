-- Phase 2: OCR derived text.
--
-- `asset_text` is rebuildable: dropping it and the OCR jobs leaves a working
-- Phase 1 library. FTS is rebuilt with an `ocr_text` column so existing search
-- finds words extracted from screenshots and documents.

CREATE TABLE IF NOT EXISTS asset_text (
  asset_id TEXT PRIMARY KEY NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  text TEXT NOT NULL,
  lang TEXT,
  confidence REAL NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_asset_text_confidence ON asset_text(confidence);

-- Rebuild FTS with OCR column. Content is repopulated from Rust after migrate.
DROP TABLE IF EXISTS assets_fts;

CREATE VIRTUAL TABLE assets_fts USING fts5(
  asset_id UNINDEXED,
  filename,
  tags,
  camera,
  lens,
  ocr_text,
  tokenize='porter'
);
