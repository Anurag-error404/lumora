-- Export history + durable activity log

CREATE TABLE IF NOT EXISTS exports (
  id TEXT PRIMARY KEY NOT NULL,
  path TEXT NOT NULL,
  asset_count INTEGER NOT NULL DEFAULT 0,
  exported_count INTEGER NOT NULL DEFAULT 0,
  missing_count INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  note TEXT
);

CREATE INDEX IF NOT EXISTS idx_exports_created_at ON exports(created_at DESC);

CREATE TABLE IF NOT EXISTS activity_log (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL,
  label TEXT NOT NULL,
  detail TEXT,
  created_at TEXT NOT NULL,
  undone INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_activity_log_created_at ON activity_log(created_at DESC);
