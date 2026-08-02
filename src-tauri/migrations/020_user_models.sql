-- User-imported (BYO) ONNX backends per AI capability.
CREATE TABLE IF NOT EXISTS ml_user_options (
  id TEXT PRIMARY KEY NOT NULL,
  capability TEXT NOT NULL,
  name TEXT NOT NULL,
  summary TEXT NOT NULL DEFAULT '',
  input_size INTEGER,
  embedding_dim INTEGER,
  primary_path TEXT NOT NULL,
  labels_path TEXT,
  text_path TEXT,
  tokenizer_path TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ml_user_options_capability
  ON ml_user_options(capability);
