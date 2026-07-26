CREATE TABLE IF NOT EXISTS asset_views (
  asset_id TEXT PRIMARY KEY NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  viewed_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_asset_views_viewed_at ON asset_views(viewed_at DESC);
