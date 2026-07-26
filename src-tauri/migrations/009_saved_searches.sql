-- Named search queries the user can reopen from the sidebar.
CREATE TABLE IF NOT EXISTS saved_searches (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL UNIQUE COLLATE NOCASE,
  query TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_saved_searches_updated_at
  ON saved_searches(updated_at DESC);
