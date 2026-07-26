-- Vault phase 2: recovery code, locked groups (albums/folders), and fully
-- encrypted item metadata.

-- A second wrapping of the same master key, under a key derived from a
-- one-time recovery code. The code itself is never stored.
ALTER TABLE vault_config ADD COLUMN recovery_salt TEXT;
ALTER TABLE vault_config ADD COLUMN recovery_nonce TEXT;
ALTER TABLE vault_config ADD COLUMN recovery_wrapped_key TEXT;

-- Albums/folders moved into the vault as a unit. The name is encrypted.
CREATE TABLE IF NOT EXISTS locked_albums (
  id TEXT PRIMARY KEY NOT NULL,
  name_enc TEXT NOT NULL,
  created_at TEXT NOT NULL
);

-- Rebuild locked_assets so every descriptive field (filename, relative path,
-- original path, media type, dimensions, size) lives inside one encrypted blob.
-- Only opaque identifiers and the locked-at timestamp stay readable, the latter
-- so the list can be ordered without decrypting every row.
-- Legacy plaintext columns are retained (nullable) purely so rows written by
-- migration 005 can be re-encrypted on the next unlock and then cleared.
CREATE TABLE locked_assets_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  vault_file TEXT NOT NULL,
  thumb_file TEXT,
  meta_enc TEXT,
  locked_album_id TEXT REFERENCES locked_albums(id) ON DELETE SET NULL,
  locked_at TEXT NOT NULL,
  legacy_file_name TEXT,
  legacy_media_type TEXT,
  legacy_width INTEGER,
  legacy_height INTEGER,
  legacy_size_bytes INTEGER,
  legacy_original_path TEXT
);

INSERT INTO locked_assets_v2 (
  id, vault_file, thumb_file, meta_enc, locked_album_id, locked_at,
  legacy_file_name, legacy_media_type, legacy_width, legacy_height,
  legacy_size_bytes, legacy_original_path
)
SELECT
  id, vault_file, thumb_file, NULL, NULL, locked_at,
  file_name, media_type, width, height, size_bytes, original_path
FROM locked_assets;

DROP TABLE locked_assets;
ALTER TABLE locked_assets_v2 RENAME TO locked_assets;

CREATE INDEX IF NOT EXISTS idx_locked_assets_locked_at ON locked_assets(locked_at);
CREATE INDEX IF NOT EXISTS idx_locked_assets_album ON locked_assets(locked_album_id);
