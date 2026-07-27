-- Phase 3: non-destructive edit history (ops sidecars).
-- Original files stay untouched until the user explicitly bakes (replace/copy).
CREATE TABLE IF NOT EXISTS asset_edits (
  id TEXT PRIMARY KEY NOT NULL,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  ops_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_asset_edits_asset
  ON asset_edits(asset_id, created_at DESC);
