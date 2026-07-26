-- Phase 2: MobileNetV4 auto-tags + import performance runs.
--
-- `asset_labels` is rebuildable. FTS gains an `auto_tags` column so classifier
-- labels are searchable without polluting user tags.
-- `import_runs` stores local-only import timing for Developer / Activity.

CREATE TABLE IF NOT EXISTS asset_labels (
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  label TEXT NOT NULL,
  score REAL NOT NULL DEFAULT 0,
  rank INTEGER NOT NULL DEFAULT 0,
  model_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (asset_id, label)
);

CREATE INDEX IF NOT EXISTS idx_asset_labels_asset ON asset_labels(asset_id);
CREATE INDEX IF NOT EXISTS idx_asset_labels_label ON asset_labels(label);

CREATE TABLE IF NOT EXISTS import_runs (
  id TEXT PRIMARY KEY NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  scanned INTEGER NOT NULL,
  inserted INTEGER NOT NULL,
  updated INTEGER NOT NULL,
  skipped INTEGER NOT NULL,
  cancelled INTEGER NOT NULL DEFAULT 0,
  files_per_sec REAL,
  roots_json TEXT,
  note TEXT
);

CREATE INDEX IF NOT EXISTS idx_import_runs_finished ON import_runs(finished_at DESC);

-- Rebuild FTS with auto_tags. Content is repopulated from Rust after migrate.
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
  tokenize='porter'
);
