-- Privacy vault: encrypted "locked folder" configuration and contents.

-- Single-row configuration for the encrypted vault. Stores the wrapped master
-- key material (never the password) and the KDF parameters used to derive the
-- key-encryption key from the password.
CREATE TABLE IF NOT EXISTS vault_config (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  vault_path TEXT NOT NULL,
  salt TEXT NOT NULL,
  wrap_nonce TEXT NOT NULL,
  wrapped_key TEXT NOT NULL,
  kdf_m_cost INTEGER NOT NULL,
  kdf_t_cost INTEGER NOT NULL,
  kdf_p_cost INTEGER NOT NULL,
  created_at TEXT NOT NULL
);

-- Metadata for each item moved into the vault. The original bytes live only as
-- encrypted blobs inside `vault_path`; each per-file nonce is stored in the
-- blob header, so no key material is kept here.
CREATE TABLE IF NOT EXISTS locked_assets (
  id TEXT PRIMARY KEY NOT NULL,
  vault_file TEXT NOT NULL,
  thumb_file TEXT,
  file_name TEXT NOT NULL,
  media_type TEXT NOT NULL,
  width INTEGER,
  height INTEGER,
  size_bytes INTEGER,
  original_path TEXT,
  locked_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_locked_assets_locked_at ON locked_assets(locked_at);
