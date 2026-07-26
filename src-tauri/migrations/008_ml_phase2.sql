-- Phase 2: on-device intelligence.
--
-- Every table here holds DERIVED data. Dropping all of them must leave a fully
-- working Phase 1 library, so nothing outside the ml/semantic/faces/ocr modules
-- may depend on these rows existing.

-- Registry of locally installed models. A model is only usable once its file
-- hashes to the pinned sha256, so a truncated or tampered download can never
-- be loaded.
CREATE TABLE IF NOT EXISTS ml_models (
  id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL,
  version TEXT NOT NULL,
  path TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  size_bytes INTEGER NOT NULL DEFAULT 0,
  -- Embedding width, where the model produces vectors.
  dim INTEGER,
  installed_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ml_models_kind ON ml_models(kind);

-- One vector per (asset, model). Keyed by model so a future model upgrade can
-- be backfilled alongside the old vectors instead of invalidating the library.
CREATE TABLE IF NOT EXISTS asset_embeddings (
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  model_id TEXT NOT NULL,
  dim INTEGER NOT NULL,
  -- L2-normalised f32, little-endian. Because vectors are unit length,
  -- cosine similarity reduces to a dot product at query time.
  vector BLOB NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (asset_id, model_id)
);

CREATE INDEX IF NOT EXISTS idx_asset_embeddings_model ON asset_embeddings(model_id);

-- Per-asset, per-capability processing state. This is what makes inference
-- resumable: an interrupted run leaves rows in 'pending' rather than losing
-- track of which assets were already handled.
CREATE TABLE IF NOT EXISTS ml_jobs (
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('pending', 'done', 'failed', 'skipped')),
  attempts INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (asset_id, kind)
);

CREATE INDEX IF NOT EXISTS idx_ml_jobs_state ON ml_jobs(kind, state);
