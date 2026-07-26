-- Multi-vault: replace the single-row vault_config with a vaults table, and
-- scope locked albums/assets to a vault_id.
--
-- Data migration (copy vault_config → vaults, backfill vault_id) runs in Rust
-- immediately after this schema change so we can assign a proper UUID.

CREATE TABLE IF NOT EXISTS vaults (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  vault_path TEXT NOT NULL UNIQUE,
  salt TEXT NOT NULL,
  wrap_nonce TEXT NOT NULL,
  wrapped_key TEXT NOT NULL,
  kdf_m_cost INTEGER NOT NULL,
  kdf_t_cost INTEGER NOT NULL,
  kdf_p_cost INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  recovery_salt TEXT,
  recovery_nonce TEXT,
  recovery_wrapped_key TEXT
);

-- locked_albums gains vault_id; rebuild so the column is NOT NULL.
CREATE TABLE locked_albums_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  vault_id TEXT NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
  name_enc TEXT NOT NULL,
  created_at TEXT NOT NULL
);

-- locked_assets gains vault_id; keep legacy plaintext columns for older rows.
CREATE TABLE locked_assets_v3 (
  id TEXT PRIMARY KEY NOT NULL,
  vault_id TEXT NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
  vault_file TEXT NOT NULL,
  thumb_file TEXT,
  meta_enc TEXT,
  locked_album_id TEXT REFERENCES locked_albums_v2(id) ON DELETE SET NULL,
  locked_at TEXT NOT NULL,
  legacy_file_name TEXT,
  legacy_media_type TEXT,
  legacy_width INTEGER,
  legacy_height INTEGER,
  legacy_size_bytes INTEGER,
  legacy_original_path TEXT
);
