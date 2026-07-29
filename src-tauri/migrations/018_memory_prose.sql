-- Memories v1.5+ optional prose cache.
-- Rebuildable: drop table anytime; templates still work without prose.

CREATE TABLE IF NOT EXISTS memory_prose (
  memory_id TEXT PRIMARY KEY NOT NULL,
  input_hash TEXT NOT NULL,
  prose TEXT NOT NULL,
  model_id TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memory_prose_hash ON memory_prose(input_hash);
