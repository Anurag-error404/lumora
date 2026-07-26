//! Privacy vault: a password-protected "locked folder" whose contents are
//! encrypted at rest.
//!
//! * Items (single files, whole albums, or whole folders on disk) are moved in:
//!   the original bytes are encrypted into the user-chosen vault folder and the
//!   plaintext original is deleted.
//! * **All descriptive metadata is encrypted too** — filenames, paths, media
//!   type and dimensions live in a single sealed blob per item, and group names
//!   are sealed as well. Only opaque IDs and the locked-at timestamp (needed to
//!   order the list without decrypting every row) remain readable.
//! * Decryption happens in memory only; nothing is ever written back as
//!   plaintext except an explicit "move out" to a folder the user picks.

pub mod crypto;
pub mod portable;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::error::{AppError, AppResult};
use crate::indexer;
use crate::models::{LockResult, LockedAlbum, LockedAsset, MoveOutResult, VaultSummary};
use crate::thumbnails;
use crypto::{KdfParams, MASTER_KEY_LEN};
use portable::{
    blobs_root, sanitize_component, unique_path, write_manifest, Catalog, CatalogAlbum,
    CatalogItem, VaultManifest, CATALOG_VERSION,
};

const MIN_PASSWORD_LEN: usize = 4;

struct RecoveryMaterial {
    salt: Vec<u8>,
    nonce: Vec<u8>,
    wrapped_key: Vec<u8>,
}

struct VaultConfig {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    name: String,
    vault_path: String,
    salt: Vec<u8>,
    wrap_nonce: Vec<u8>,
    wrapped_key: Vec<u8>,
    kdf: KdfParams,
    recovery: Option<RecoveryMaterial>,
}

/// Result of creating a vault: the new vault id, master key for the session,
/// plus the one-time recovery code (shown once, never stored).
pub struct SetupOutcome {
    pub vault_id: String,
    pub master_key: [u8; MASTER_KEY_LEN],
    pub recovery_code: String,
}

/// Everything descriptive about a locked item. Persisted only as ciphertext.
#[derive(Serialize, Deserialize, Default)]
struct LockedMeta {
    file_name: String,
    /// Path relative to the locked group root; equals `file_name` for loose items.
    rel_path: String,
    original_path: String,
    media_type: String,
    width: Option<i64>,
    height: Option<i64>,
    size_bytes: Option<i64>,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

fn decode_hex(value: &str, what: &str) -> AppResult<Vec<u8>> {
    hex::decode(value).map_err(|_| AppError::msg(format!("corrupt vault config: {what}")))
}

fn load_config(conn: &Connection, vault_id: &str) -> AppResult<Option<VaultConfig>> {
    let row = conn
        .query_row(
            "SELECT id, name, vault_path, salt, wrap_nonce, wrapped_key, kdf_m_cost, kdf_t_cost,
                    kdf_p_cost, recovery_salt, recovery_nonce, recovery_wrapped_key
             FROM vaults WHERE id = ?1",
            params![vault_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, u32>(6)?,
                    r.get::<_, u32>(7)?,
                    r.get::<_, u32>(8)?,
                    r.get::<_, Option<String>>(9)?,
                    r.get::<_, Option<String>>(10)?,
                    r.get::<_, Option<String>>(11)?,
                ))
            },
        )
        .optional()?;

    let Some((
        id,
        name,
        vault_path,
        salt,
        nonce,
        wrapped,
        m,
        t,
        p,
        rec_salt,
        rec_nonce,
        rec_wrapped,
    )) = row
    else {
        return Ok(None);
    };

    let recovery = match (rec_salt, rec_nonce, rec_wrapped) {
        (Some(s), Some(n), Some(w)) => Some(RecoveryMaterial {
            salt: decode_hex(&s, "recovery salt")?,
            nonce: decode_hex(&n, "recovery nonce")?,
            wrapped_key: decode_hex(&w, "recovery key")?,
        }),
        _ => None,
    };

    Ok(Some(VaultConfig {
        id,
        name,
        vault_path,
        salt: decode_hex(&salt, "salt")?,
        wrap_nonce: decode_hex(&nonce, "nonce")?,
        wrapped_key: decode_hex(&wrapped, "key")?,
        kdf: KdfParams {
            m_cost: m,
            t_cost: t,
            p_cost: p,
        },
        recovery,
    }))
}

fn require_config(conn: &Connection, vault_id: &str) -> AppResult<VaultConfig> {
    load_config(conn, vault_id)?.ok_or_else(|| AppError::msg("vault not found"))
}

pub fn is_configured(conn: &Connection) -> AppResult<bool> {
    Ok(conn.query_row("SELECT COUNT(*) FROM vaults", [], |r| r.get::<_, i64>(0))? > 0)
}

pub fn total_locked_count(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM locked_assets", [], |r| r.get(0))?)
}

pub fn locked_count_for(conn: &Connection, vault_id: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM locked_assets WHERE vault_id = ?1",
        params![vault_id],
        |r| r.get(0),
    )?)
}

/// Test-only probe. Production reads this flag through `list_vaults`, which
/// derives it in the same query that loads the vault row.
#[cfg(test)]
pub fn recovery_configured(conn: &Connection, vault_id: &str) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT recovery_wrapped_key IS NOT NULL FROM vaults WHERE id = ?1",
            params![vault_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false))
}

/// List all vaults. `active_vault_id` marks which one is unlocked this session.
pub fn list_vaults(
    conn: &Connection,
    active_vault_id: Option<&str>,
) -> AppResult<Vec<VaultSummary>> {
    let rows: Vec<(String, String, String, bool)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, vault_path, recovery_wrapped_key IS NOT NULL
             FROM vaults ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut out = Vec::with_capacity(rows.len());
    for (id, name, path, recovery_configured) in rows {
        let locked_count = locked_count_for(conn, &id)?;
        out.push(VaultSummary {
            unlocked: active_vault_id == Some(id.as_str()),
            id,
            name,
            path,
            locked_count,
            recovery_configured,
        });
    }
    Ok(out)
}

fn path_taken(conn: &Connection, vault_path: &str) -> AppResult<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM vaults WHERE vault_path = ?1",
            params![vault_path],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

// ---------------------------------------------------------------------------
// Metadata sealing
// ---------------------------------------------------------------------------

fn seal_bytes(key: &[u8; MASTER_KEY_LEN], plain: &[u8]) -> AppResult<String> {
    Ok(STANDARD.encode(crypto::encrypt_blob(key, plain)?))
}

fn open_bytes(key: &[u8; MASTER_KEY_LEN], sealed: &str) -> AppResult<Vec<u8>> {
    let raw = STANDARD
        .decode(sealed)
        .map_err(|_| AppError::msg("corrupt encrypted metadata"))?;
    crypto::decrypt_blob(key, &raw)
}

fn seal_meta(key: &[u8; MASTER_KEY_LEN], meta: &LockedMeta) -> AppResult<String> {
    let json = serde_json::to_vec(meta).map_err(|e| AppError::msg(e.to_string()))?;
    seal_bytes(key, &json)
}

fn open_meta(key: &[u8; MASTER_KEY_LEN], sealed: &str) -> AppResult<LockedMeta> {
    let json = open_bytes(key, sealed)?;
    serde_json::from_slice(&json).map_err(|e| AppError::msg(e.to_string()))
}

fn seal_text(key: &[u8; MASTER_KEY_LEN], text: &str) -> AppResult<String> {
    seal_bytes(key, text.as_bytes())
}

fn open_text(key: &[u8; MASTER_KEY_LEN], sealed: &str) -> AppResult<String> {
    let bytes = open_bytes(key, sealed)?;
    String::from_utf8(bytes).map_err(|_| AppError::msg("corrupt encrypted name"))
}

// ---------------------------------------------------------------------------
// Portable catalog
// ---------------------------------------------------------------------------

/// Rebuild `catalog.pv` inside the vault folder from the current DB rows so the
/// folder alone is enough to restore original names and structure elsewhere.
/// Called after every mutation; failures are surfaced to the caller's log only,
/// since a stale catalog must never block the operation itself.
fn write_catalog_from_db(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
) -> AppResult<()> {
    let config = require_config(conn, vault_id)?;

    let albums = {
        let mut stmt =
            conn.prepare("SELECT id, name_enc FROM locked_albums WHERE vault_id = ?1")?;
        let rows = stmt.query_map(params![vault_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        rows.filter_map(|r| r.ok())
            .map(|(id, name_enc)| {
                let name = match &name_enc {
                    Some(sealed) => open_text(key, sealed).unwrap_or_else(|_| "Locked".into()),
                    None => "Locked".into(),
                };
                CatalogAlbum { id, name }
            })
            .collect::<Vec<_>>()
    };

    let items = {
        let mut stmt = conn.prepare(
            "SELECT id, vault_file, locked_album_id, meta_enc
             FROM locked_assets WHERE vault_id = ?1",
        )?;
        let rows = stmt.query_map(params![vault_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        rows.filter_map(|r| r.ok())
            .map(|(id, vault_file, album_id, meta_enc)| {
                let meta = meta_enc
                    .as_deref()
                    .and_then(|sealed| open_meta(key, sealed).ok())
                    .unwrap_or_default();
                let file_name = if meta.file_name.is_empty() {
                    "file".to_string()
                } else {
                    meta.file_name
                };
                let rel_path = if meta.rel_path.is_empty() {
                    file_name.clone()
                } else {
                    meta.rel_path
                };
                CatalogItem {
                    id,
                    vault_file,
                    album_id,
                    file_name,
                    rel_path,
                }
            })
            .collect::<Vec<_>>()
    };

    portable::write_catalog(
        &config.vault_path,
        key,
        &Catalog {
            version: CATALOG_VERSION,
            albums,
            items,
        },
    )
}

/// Best-effort catalog refresh: log and continue if the vault folder is
/// unavailable (unplugged drive, permissions) so vault edits still succeed.
fn refresh_catalog(conn: &Connection, vault_id: &str, key: &[u8; MASTER_KEY_LEN]) {
    if let Err(e) = write_catalog_from_db(conn, vault_id, key) {
        tracing::warn!(error = %e, "could not update portable vault catalog");
    }
}

/// `(id, file_name, media_type, width, height, size_bytes, original_path)`
type LegacyMetaRow = (
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

/// Re-encrypt rows written before metadata sealing existed (migration 005),
/// then clear the plaintext columns. Runs on every unlock; a no-op once done.
fn migrate_legacy_metadata(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
) -> AppResult<usize> {
    let legacy: Vec<LegacyMetaRow> = {
        let mut stmt = conn.prepare(
            "SELECT id, legacy_file_name, legacy_media_type, legacy_width, legacy_height,
                    legacy_size_bytes, legacy_original_path
             FROM locked_assets WHERE vault_id = ?1 AND meta_enc IS NULL",
        )?;
        let rows = stmt.query_map(params![vault_id], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut migrated = 0usize;
    for (id, file_name, media_type, width, height, size_bytes, original_path) in legacy {
        let file_name = file_name.unwrap_or_else(|| "file".into());
        let meta = LockedMeta {
            rel_path: file_name.clone(),
            file_name,
            original_path: original_path.unwrap_or_default(),
            media_type: media_type.unwrap_or_else(|| "image".into()),
            width,
            height,
            size_bytes,
        };
        conn.execute(
            "UPDATE locked_assets
             SET meta_enc = ?1, legacy_file_name = NULL, legacy_media_type = NULL,
                 legacy_width = NULL, legacy_height = NULL, legacy_size_bytes = NULL,
                 legacy_original_path = NULL
             WHERE id = ?2",
            params![seal_meta(key, &meta)?, id],
        )?;
        migrated += 1;
    }
    if migrated > 0 {
        // The old values may otherwise survive in SQLite free pages or the WAL.
        // This one-time scrub runs only for vaults created before metadata
        // encryption was introduced.
        conn.execute_batch(
            "PRAGMA secure_delete = ON;
             PRAGMA wal_checkpoint(TRUNCATE);
             VACUUM;",
        )?;
        tracing::info!(migrated, "encrypted metadata for pre-existing locked items");
    }
    Ok(migrated)
}

// ---------------------------------------------------------------------------
// Setup / unlock / recovery
// ---------------------------------------------------------------------------

/// Create a new named vault. Returns the vault id, master key for the session,
/// and a one-time recovery code.
pub fn setup(
    conn: &Connection,
    name: &str,
    vault_path: &str,
    password: &str,
) -> AppResult<SetupOutcome> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("give this vault a name"));
    }
    if vault_path.trim().is_empty() {
        return Err(AppError::msg("choose a destination folder for the vault"));
    }
    if path_taken(conn, vault_path)? {
        return Err(AppError::msg(
            "another vault already uses that destination folder",
        ));
    }
    if password.len() < MIN_PASSWORD_LEN {
        return Err(AppError::msg(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    std::fs::create_dir_all(blobs_root(vault_path))?;

    let kdf = KdfParams::default();
    let master_key = crypto::random_master_key()?;

    let salt = crypto::random_salt()?;
    let (wrap_nonce, wrapped_key) = crypto::wrap_master_key(password, &salt, kdf, &master_key)?;

    // A second, independent wrapping of the same key under the recovery code.
    let recovery_code = crypto::generate_recovery_code()?;
    let recovery_salt = crypto::random_salt()?;
    let (recovery_nonce, recovery_wrapped) = crypto::wrap_master_key(
        &crypto::normalize_recovery_code(&recovery_code),
        &recovery_salt,
        kdf,
        &master_key,
    )?;

    let vault_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO vaults
           (id, name, vault_path, salt, wrap_nonce, wrapped_key, kdf_m_cost, kdf_t_cost, kdf_p_cost,
            created_at, recovery_salt, recovery_nonce, recovery_wrapped_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            vault_id,
            name,
            vault_path,
            hex::encode(salt),
            hex::encode(&wrap_nonce),
            hex::encode(&wrapped_key),
            kdf.m_cost,
            kdf.t_cost,
            kdf.p_cost,
            created_at,
            hex::encode(recovery_salt),
            hex::encode(&recovery_nonce),
            hex::encode(&recovery_wrapped),
        ],
    )?;

    write_manifest(
        vault_path,
        &VaultManifest {
            version: 2,
            salt: hex::encode(salt),
            wrap_nonce: hex::encode(&wrap_nonce),
            wrapped_key: hex::encode(&wrapped_key),
            kdf_m_cost: kdf.m_cost,
            kdf_t_cost: kdf.t_cost,
            kdf_p_cost: kdf.p_cost,
            recovery_salt: Some(hex::encode(recovery_salt)),
            recovery_nonce: Some(hex::encode(&recovery_nonce)),
            recovery_wrapped_key: Some(hex::encode(&recovery_wrapped)),
            created_at,
        },
    );

    refresh_catalog(conn, &vault_id, &master_key);

    tracing::info!(vault = %vault_path, name = %name, "privacy vault configured");
    Ok(SetupOutcome {
        vault_id,
        master_key,
        recovery_code,
    })
}

/// Verify the password and return the master key for an unlocked session.
pub fn unlock(
    conn: &Connection,
    vault_id: &str,
    password: &str,
) -> AppResult<[u8; MASTER_KEY_LEN]> {
    let config = require_config(conn, vault_id)?;
    let key = crypto::unwrap_master_key(
        password,
        &config.salt,
        config.kdf,
        &config.wrap_nonce,
        &config.wrapped_key,
    )?;
    migrate_legacy_metadata(conn, vault_id, &key)?;
    // Vaults created before the portable catalog existed get one on first
    // unlock, so the folder alone becomes enough to restore its contents.
    if !portable::catalog_exists(&config.vault_path) {
        refresh_catalog(conn, vault_id, &key);
    }
    Ok(key)
}

/// Add recovery to a vault created before recovery codes were introduced.
/// The code is returned once and never persisted.
pub fn enable_recovery(
    conn: &Connection,
    vault_id: &str,
    master_key: &[u8; MASTER_KEY_LEN],
) -> AppResult<String> {
    let config = require_config(conn, vault_id)?;
    if config.recovery.is_some() {
        return Err(AppError::msg("recovery is already configured"));
    }

    let code = crypto::generate_recovery_code()?;
    let salt = crypto::random_salt()?;
    let (nonce, wrapped) = crypto::wrap_master_key(
        &crypto::normalize_recovery_code(&code),
        &salt,
        config.kdf,
        master_key,
    )?;
    conn.execute(
        "UPDATE vaults
         SET recovery_salt = ?1, recovery_nonce = ?2, recovery_wrapped_key = ?3
         WHERE id = ?4",
        params![
            hex::encode(salt),
            hex::encode(&nonce),
            hex::encode(&wrapped),
            vault_id,
        ],
    )?;

    write_manifest(
        &config.vault_path,
        &VaultManifest {
            version: 2,
            salt: hex::encode(&config.salt),
            wrap_nonce: hex::encode(&config.wrap_nonce),
            wrapped_key: hex::encode(&config.wrapped_key),
            kdf_m_cost: config.kdf.m_cost,
            kdf_t_cost: config.kdf.t_cost,
            kdf_p_cost: config.kdf.p_cost,
            recovery_salt: Some(hex::encode(salt)),
            recovery_nonce: Some(hex::encode(&nonce)),
            recovery_wrapped_key: Some(hex::encode(&wrapped)),
            created_at: Utc::now().to_rfc3339(),
        },
    );
    Ok(code)
}

/// Recover access with the one-time recovery code and set a new password.
/// The recovery code itself stays valid — only the password wrapping changes.
pub fn recover(
    conn: &Connection,
    vault_id: &str,
    recovery_code: &str,
    new_password: &str,
) -> AppResult<[u8; MASTER_KEY_LEN]> {
    let config = require_config(conn, vault_id)?;
    let recovery = config
        .recovery
        .as_ref()
        .ok_or_else(|| AppError::msg("this vault has no recovery code"))?;
    if new_password.len() < MIN_PASSWORD_LEN {
        return Err(AppError::msg(format!(
            "new password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }

    let normalized = crypto::normalize_recovery_code(recovery_code);
    let master_key = crypto::unwrap_master_key(
        &normalized,
        &recovery.salt,
        config.kdf,
        &recovery.nonce,
        &recovery.wrapped_key,
    )
    .map_err(|_| AppError::msg("incorrect recovery code"))?;

    // Re-wrap the same master key under the new password.
    let salt = crypto::random_salt()?;
    let (wrap_nonce, wrapped_key) =
        crypto::wrap_master_key(new_password, &salt, config.kdf, &master_key)?;
    conn.execute(
        "UPDATE vaults SET salt = ?1, wrap_nonce = ?2, wrapped_key = ?3 WHERE id = ?4",
        params![
            hex::encode(salt),
            hex::encode(&wrap_nonce),
            hex::encode(&wrapped_key),
            vault_id,
        ],
    )?;

    write_manifest(
        &config.vault_path,
        &VaultManifest {
            version: 2,
            salt: hex::encode(salt),
            wrap_nonce: hex::encode(&wrap_nonce),
            wrapped_key: hex::encode(&wrapped_key),
            kdf_m_cost: config.kdf.m_cost,
            kdf_t_cost: config.kdf.t_cost,
            kdf_p_cost: config.kdf.p_cost,
            recovery_salt: Some(hex::encode(&recovery.salt)),
            recovery_nonce: Some(hex::encode(&recovery.nonce)),
            recovery_wrapped_key: Some(hex::encode(&recovery.wrapped_key)),
            created_at: Utc::now().to_rfc3339(),
        },
    );

    migrate_legacy_metadata(conn, vault_id, &master_key)?;
    tracing::info!(vault_id, "vault password reset via recovery code");
    Ok(master_key)
}

// ---------------------------------------------------------------------------
// Locking items in
// ---------------------------------------------------------------------------

/// One file about to be moved into the vault, with everything needed to seal it.
struct SourceItem {
    path: PathBuf,
    rel_path: String,
    media_type: String,
    width: Option<i64>,
    height: Option<i64>,
    size_bytes: Option<i64>,
    /// Existing thumbnail on disk to encrypt and then remove, if any.
    thumb_source: Option<PathBuf>,
    /// Library row to delete once the file is safely in the vault.
    asset_id: Option<String>,
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into())
}

/// Encrypt one file into the vault and drop its library row + plaintext copies.
fn lock_item(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    blobs: &Path,
    item: &SourceItem,
    album_id: Option<&str>,
    errors: &mut Vec<String>,
) -> AppResult<bool> {
    let plaintext = match std::fs::read(&item.path) {
        Ok(bytes) => bytes,
        Err(e) => {
            errors.push(format!("{}: {e}", item.path.display()));
            return Ok(false);
        }
    };

    let lock_id = Uuid::new_v4().to_string();
    let vault_file = format!("{lock_id}.pv");
    let blob = crypto::encrypt_blob(key, &plaintext)?;
    if let Err(e) = std::fs::write(blobs.join(&vault_file), &blob) {
        errors.push(format!(
            "{}: could not write vault file: {e}",
            item.path.display()
        ));
        return Ok(false);
    }

    // Prefer an existing thumbnail; otherwise render one in memory so no
    // plaintext preview is ever written to disk.
    let thumb_bytes = item
        .thumb_source
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .or_else(|| {
            if item.media_type == "image" {
                thumbnails::thumbnail_bytes(&item.path).ok()
            } else {
                None
            }
        });
    let mut thumb_file: Option<String> = None;
    if let Some(bytes) = thumb_bytes {
        if let Ok(sealed) = crypto::encrypt_blob(key, &bytes) {
            let name = format!("{lock_id}.pt");
            if std::fs::write(blobs.join(&name), &sealed).is_ok() {
                thumb_file = Some(name);
            }
        }
    }

    let meta = LockedMeta {
        file_name: file_name_of(&item.path),
        rel_path: item.rel_path.clone(),
        original_path: item.path.display().to_string(),
        media_type: item.media_type.clone(),
        width: item.width,
        height: item.height,
        size_bytes: item.size_bytes.or(Some(plaintext.len() as i64)),
    };

    conn.execute(
        "INSERT INTO locked_assets
           (id, vault_id, vault_file, thumb_file, meta_enc, locked_album_id, locked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            lock_id,
            vault_id,
            vault_file,
            thumb_file,
            seal_meta(key, &meta)?,
            album_id,
            Utc::now().to_rfc3339(),
        ],
    )?;

    // Remove the plaintext original — this is the "move" into the vault.
    match std::fs::remove_file(&item.path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => errors.push(format!(
            "{}: original left on disk: {e}",
            item.path.display()
        )),
    }

    if let Some(asset_id) = &item.asset_id {
        // Drop the unencrypted thumbnail unless another asset still shares it.
        if let Some(thumb) = &item.thumb_source {
            let thumb_str = thumb.display().to_string();
            let still_used: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM assets WHERE thumbnail_path = ?1 AND id != ?2",
                    params![thumb_str, asset_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if still_used == 0 {
                let _ = std::fs::remove_file(thumb);
            }
        }
        conn.execute(
            "DELETE FROM assets_fts WHERE asset_id = ?1",
            params![asset_id],
        )?;
        conn.execute("DELETE FROM assets WHERE id = ?1", params![asset_id])?;
    }

    Ok(true)
}

/// Build source items from library assets (each becomes a loose vault item).
fn collect_from_assets(conn: &Connection, ids: &[String]) -> AppResult<Vec<SourceItem>> {
    let mut items = Vec::new();
    for id in ids {
        let row = conn
            .query_row(
                "SELECT path, media_type, width, height, file_size, thumbnail_path
                 FROM assets WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                        r.get::<_, Option<i64>>(4)?,
                        r.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((path, media_type, width, height, size_bytes, thumb)) = row else {
            continue;
        };
        let path = PathBuf::from(path);
        items.push(SourceItem {
            rel_path: file_name_of(&path),
            path,
            media_type,
            width,
            height,
            size_bytes,
            thumb_source: thumb.map(PathBuf::from),
            asset_id: Some(id.clone()),
        });
    }
    Ok(items)
}

/// Walk a folder on disk for supported media, reusing library rows where the
/// file is already indexed so dimensions/thumbnails aren't recomputed.
fn collect_from_folder(conn: &Connection, root: &Path) -> AppResult<Vec<SourceItem>> {
    let mut items = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(kind) = indexer::media_type_for_path(path) else {
            continue;
        };
        let rel_path = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_name_of(path));

        let indexed = conn
            .query_row(
                "SELECT id, width, height, file_size, thumbnail_path
                 FROM assets WHERE path = ?1 AND deleted_at IS NULL",
                params![path.display().to_string()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?;

        let size_on_disk = std::fs::metadata(path).ok().map(|m| m.len() as i64);
        let item = match indexed {
            Some((asset_id, width, height, size_bytes, thumb)) => SourceItem {
                path: path.to_path_buf(),
                rel_path,
                media_type: kind.as_str().to_string(),
                width,
                height,
                size_bytes: size_bytes.or(size_on_disk),
                thumb_source: thumb.map(PathBuf::from),
                asset_id: Some(asset_id),
            },
            None => {
                let (width, height) = image::image_dimensions(path)
                    .map(|(w, h)| (Some(w as i64), Some(h as i64)))
                    .unwrap_or((None, None));
                SourceItem {
                    path: path.to_path_buf(),
                    rel_path,
                    media_type: kind.as_str().to_string(),
                    width,
                    height,
                    size_bytes: size_on_disk,
                    thumb_source: None,
                    asset_id: None,
                }
            }
        };
        items.push(item);
    }
    Ok(items)
}

fn create_locked_album(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    name: &str,
) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO locked_albums (id, vault_id, name_enc, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, vault_id, seal_text(key, name)?, Utc::now().to_rfc3339()],
    )?;
    Ok(id)
}

fn lock_all(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    items: &[SourceItem],
    album_id: Option<&str>,
) -> AppResult<LockResult> {
    let config = require_config(conn, vault_id)?;
    let blobs = blobs_root(&config.vault_path);
    std::fs::create_dir_all(&blobs)?;

    let mut locked = 0usize;
    let mut errors = Vec::new();
    for item in items {
        if lock_item(conn, vault_id, key, &blobs, item, album_id, &mut errors)? {
            locked += 1;
        }
    }
    refresh_catalog(conn, vault_id, key);
    Ok(LockResult {
        locked,
        album_id: album_id.map(|s| s.to_string()),
        errors,
    })
}

/// Move individual library assets into the vault as loose items.
pub fn lock_assets(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    ids: &[String],
) -> AppResult<LockResult> {
    let items = collect_from_assets(conn, ids)?;
    let result = lock_all(conn, vault_id, key, &items, None)?;
    tracing::info!(
        locked = result.locked,
        errors = result.errors.len(),
        "locked assets into vault"
    );
    Ok(result)
}

/// Move an entire library album into the vault, preserving it as a group.
pub fn lock_album(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    album_id: &str,
) -> AppResult<LockResult> {
    let name: String = conn
        .query_row(
            "SELECT name FROM albums WHERE id = ?1",
            params![album_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::msg("album not found"))?;

    let asset_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT a.id FROM assets a
             JOIN album_assets aa ON aa.asset_id = a.id
             WHERE aa.album_id = ?1 AND a.deleted_at IS NULL",
        )?;
        let rows = stmt.query_map(params![album_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    if asset_ids.is_empty() {
        return Err(AppError::msg("this album has no photos to lock"));
    }

    let locked_album_id = create_locked_album(conn, vault_id, key, &name)?;
    let items = collect_from_assets(conn, &asset_ids)?;
    let result = lock_all(conn, vault_id, key, &items, Some(&locked_album_id))?;

    if result.locked == items.len() {
        // The library album is now empty; remove it so it isn't left as a stub.
        conn.execute("DELETE FROM albums WHERE id = ?1", params![album_id])?;
    } else if result.locked == 0 {
        conn.execute(
            "DELETE FROM locked_albums WHERE id = ?1",
            params![locked_album_id],
        )?;
        refresh_catalog(conn, vault_id, key);
    }

    tracing::info!(locked = result.locked, "locked album into vault");
    Ok(result)
}

/// Move an entire folder from disk into the vault, preserving its name as a
/// group and its internal structure via each item's relative path.
pub fn lock_folder(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    root: &Path,
) -> AppResult<LockResult> {
    if !root.is_dir() {
        return Err(AppError::msg(format!("not a folder: {}", root.display())));
    }
    // Refuse to swallow the vault itself.
    let config = require_config(conn, vault_id)?;
    if root.starts_with(Path::new(&config.vault_path)) {
        return Err(AppError::msg("that folder is inside the vault destination"));
    }

    let items = collect_from_folder(conn, root)?;
    if items.is_empty() {
        return Err(AppError::msg("no photos or videos found in that folder"));
    }

    let name = root
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Folder".into());
    let album_id = create_locked_album(conn, vault_id, key, &name)?;
    let result = lock_all(conn, vault_id, key, &items, Some(&album_id))?;
    if result.locked == 0 {
        conn.execute("DELETE FROM locked_albums WHERE id = ?1", params![album_id])?;
        refresh_catalog(conn, vault_id, key);
    }

    // Clean up directories left empty by the move (never removes anything
    // that still holds files the user cares about).
    for entry in WalkDir::new(root)
        .contents_first(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            let _ = std::fs::remove_dir(entry.path());
        }
    }

    tracing::info!(locked = result.locked, folder = %root.display(), "locked folder into vault");
    Ok(result)
}

// ---------------------------------------------------------------------------
// Reading vault contents
// ---------------------------------------------------------------------------

pub fn list_locked_albums(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
) -> AppResult<Vec<LockedAlbum>> {
    let rows: Vec<(String, String, String, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT g.id, g.name_enc, g.created_at,
                    (SELECT COUNT(*) FROM locked_assets i WHERE i.locked_album_id = g.id)
             FROM locked_albums g
             WHERE g.vault_id = ?1
             ORDER BY g.created_at DESC",
        )?;
        let rows = stmt.query_map(params![vault_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut albums = Vec::with_capacity(rows.len());
    for (id, name_enc, created_at, item_count) in rows {
        albums.push(LockedAlbum {
            id,
            name: open_text(key, &name_enc)?,
            item_count,
            created_at,
        });
    }
    Ok(albums)
}

/// `(id, meta_enc, thumb_file, locked_album_id, locked_at)`
type LockedRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);

/// List locked items. `album_id = None` returns only loose items (those not in
/// a locked group), matching how the UI shows groups as folders.
pub fn list_locked(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    album_id: Option<&str>,
) -> AppResult<Vec<LockedAsset>> {
    let rows: Vec<LockedRow> = match album_id {
        Some(id) => {
            let mut stmt = conn.prepare(
                "SELECT id, meta_enc, thumb_file, locked_album_id, locked_at
                 FROM locked_assets
                 WHERE vault_id = ?1 AND locked_album_id = ?2
                 ORDER BY locked_at DESC",
            )?;
            let rows = stmt.query_map(params![vault_id, id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?;
            rows.filter_map(|r| r.ok()).collect()
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT id, meta_enc, thumb_file, locked_album_id, locked_at
                 FROM locked_assets
                 WHERE vault_id = ?1 AND locked_album_id IS NULL
                 ORDER BY locked_at DESC",
            )?;
            let rows = stmt.query_map(params![vault_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?;
            rows.filter_map(|r| r.ok()).collect()
        }
    };

    let mut items = Vec::with_capacity(rows.len());
    for (id, meta_enc, thumb_file, album, locked_at) in rows {
        let meta = match &meta_enc {
            Some(sealed) => open_meta(key, sealed)?,
            None => LockedMeta::default(),
        };
        items.push(LockedAsset {
            id,
            file_name: meta.file_name,
            rel_path: meta.rel_path,
            media_type: meta.media_type,
            width: meta.width,
            height: meta.height,
            size_bytes: meta.size_bytes,
            has_thumb: thumb_file.is_some(),
            album_id: album,
            locked_at,
        });
    }
    Ok(items)
}

fn read_blob(vault_path: &str, file: &str) -> AppResult<Vec<u8>> {
    std::fs::read(blobs_root(vault_path).join(file))
        .map_err(|e| AppError::msg(format!("vault file unavailable: {e}")))
}

/// Decrypt a locked item's thumbnail into a base64 data URL (in-memory only).
pub fn decrypt_thumb(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    id: &str,
) -> AppResult<Option<String>> {
    let config = require_config(conn, vault_id)?;
    let thumb_file: Option<String> = conn
        .query_row(
            "SELECT thumb_file FROM locked_assets WHERE id = ?1 AND vault_id = ?2",
            params![id, vault_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let Some(thumb_file) = thumb_file else {
        return Ok(None);
    };
    let blob = read_blob(&config.vault_path, &thumb_file)?;
    let bytes = crypto::decrypt_blob(key, &blob)?;
    Ok(Some(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(bytes)
    )))
}

/// Decrypt a locked item's full media into a base64 data URL (in-memory only).
pub fn decrypt_media(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    id: &str,
) -> AppResult<String> {
    let config = require_config(conn, vault_id)?;
    let (vault_file, meta_enc) = conn
        .query_row(
            "SELECT vault_file, meta_enc FROM locked_assets WHERE id = ?1 AND vault_id = ?2",
            params![id, vault_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()?
        .ok_or_else(|| AppError::msg("locked item not found"))?;

    let meta = match &meta_enc {
        Some(sealed) => open_meta(key, sealed)?,
        None => LockedMeta::default(),
    };
    let blob = read_blob(&config.vault_path, &vault_file)?;
    let bytes = crypto::decrypt_blob(key, &blob)?;
    let mime = guess_mime(&meta.file_name, &meta.media_type);
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

// ---------------------------------------------------------------------------
// Moving out / deleting
// ---------------------------------------------------------------------------

/// Decrypt selected items back to a destination folder and remove them from the
/// vault. Items that were locked as part of a group are restored under a
/// subfolder named after that group, preserving their original structure.
pub fn move_out(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    ids: &[String],
    dest_dir: &str,
) -> AppResult<MoveOutResult> {
    let config = require_config(conn, vault_id)?;
    if dest_dir.trim().is_empty() {
        return Err(AppError::msg("choose a destination folder"));
    }
    let dest = Path::new(dest_dir);
    std::fs::create_dir_all(dest)?;

    let mut album_names: HashMap<String, String> = HashMap::new();
    let mut restored = 0usize;
    let mut paths = Vec::new();
    let mut errors = Vec::new();

    for id in ids {
        let row = conn
            .query_row(
                "SELECT vault_file, thumb_file, meta_enc, locked_album_id
                 FROM locked_assets WHERE id = ?1 AND vault_id = ?2",
                params![id, vault_id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((vault_file, thumb_file, meta_enc, album)) = row else {
            errors.push(format!("{id}: not in vault"));
            continue;
        };

        let meta = match &meta_enc {
            Some(sealed) => open_meta(key, sealed)?,
            None => LockedMeta::default(),
        };
        let blob = match read_blob(&config.vault_path, &vault_file) {
            Ok(b) => b,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let bytes = match crypto::decrypt_blob(key, &blob) {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{}: {e}", meta.file_name));
                continue;
            }
        };

        // Group members land under <dest>/<group name>/<relative path>.
        let mut target_dir = dest.to_path_buf();
        if let Some(album_id) = &album {
            let name = match album_names.get(album_id) {
                Some(name) => name.clone(),
                None => {
                    let sealed: Option<String> = conn
                        .query_row(
                            "SELECT name_enc FROM locked_albums WHERE id = ?1 AND vault_id = ?2",
                            params![album_id, vault_id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    let name = match sealed {
                        Some(s) => open_text(key, &s)?,
                        None => "Locked".to_string(),
                    };
                    album_names.insert(album_id.clone(), name.clone());
                    name
                }
            };
            target_dir = target_dir.join(sanitize_component(&name));
        }

        let rel = if meta.rel_path.is_empty() {
            meta.file_name.clone()
        } else {
            meta.rel_path.clone()
        };
        if let Some(parent) = Path::new(&rel).parent() {
            if !parent.as_os_str().is_empty() {
                target_dir = target_dir.join(parent);
            }
        }
        if let Err(e) = std::fs::create_dir_all(&target_dir) {
            errors.push(format!("{}: {e}", meta.file_name));
            continue;
        }

        let leaf = Path::new(&rel)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| meta.file_name.clone());
        let target = unique_path(&target_dir, &leaf);
        if let Err(e) = std::fs::write(&target, &bytes) {
            errors.push(format!("{}: {e}", meta.file_name));
            continue;
        }

        // Remove blobs + metadata now the plaintext is safely written out.
        let _ = std::fs::remove_file(blobs_root(&config.vault_path).join(&vault_file));
        if let Some(tf) = thumb_file {
            let _ = std::fs::remove_file(blobs_root(&config.vault_path).join(tf));
        }
        conn.execute(
            "DELETE FROM locked_assets WHERE id = ?1 AND vault_id = ?2",
            params![id, vault_id],
        )?;
        paths.push(target.display().to_string());
        restored += 1;
    }

    prune_empty_albums(conn, vault_id)?;
    refresh_catalog(conn, vault_id, key);
    tracing::info!(restored, errors = errors.len(), "moved items out of vault");
    Ok(MoveOutResult {
        restored,
        paths,
        errors,
    })
}

/// Move an entire locked group out to a destination folder.
pub fn move_out_album(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    album_id: &str,
    dest_dir: &str,
) -> AppResult<MoveOutResult> {
    let ids = album_item_ids(conn, vault_id, album_id)?;
    if ids.is_empty() {
        return Err(AppError::msg("this locked folder is empty"));
    }
    move_out(conn, vault_id, key, &ids, dest_dir)
}

fn album_item_ids(conn: &Connection, vault_id: &str, album_id: &str) -> AppResult<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM locked_assets WHERE vault_id = ?1 AND locked_album_id = ?2")?;
    let rows = stmt.query_map(params![vault_id, album_id], |r| r.get(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Drop group rows that no longer have any items.
fn prune_empty_albums(conn: &Connection, vault_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM locked_albums
         WHERE vault_id = ?1 AND NOT EXISTS (
           SELECT 1 FROM locked_assets i WHERE i.locked_album_id = locked_albums.id
         )",
        params![vault_id],
    )?;
    Ok(())
}

/// Permanently delete locked items (encrypted blobs + metadata) without
/// restoring them. Irreversible.
pub fn delete_locked(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    ids: &[String],
) -> AppResult<usize> {
    let config = require_config(conn, vault_id)?;
    let mut removed = 0usize;
    for id in ids {
        let row = conn
            .query_row(
                "SELECT vault_file, thumb_file FROM locked_assets WHERE id = ?1 AND vault_id = ?2",
                params![id, vault_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        if let Some((vault_file, thumb_file)) = row {
            let _ = std::fs::remove_file(blobs_root(&config.vault_path).join(&vault_file));
            if let Some(tf) = thumb_file {
                let _ = std::fs::remove_file(blobs_root(&config.vault_path).join(tf));
            }
            removed += conn.execute(
                "DELETE FROM locked_assets WHERE id = ?1 AND vault_id = ?2",
                params![id, vault_id],
            )?;
        }
    }
    prune_empty_albums(conn, vault_id)?;
    refresh_catalog(conn, vault_id, key);
    Ok(removed)
}

/// Permanently delete an entire locked group and everything in it.
pub fn delete_locked_album(
    conn: &Connection,
    vault_id: &str,
    key: &[u8; MASTER_KEY_LEN],
    album_id: &str,
) -> AppResult<usize> {
    let ids = album_item_ids(conn, vault_id, album_id)?;
    let removed = delete_locked(conn, vault_id, key, &ids)?;
    conn.execute(
        "DELETE FROM locked_albums WHERE id = ?1 AND vault_id = ?2",
        params![album_id, vault_id],
    )?;
    refresh_catalog(conn, vault_id, key);
    Ok(removed)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn guess_mime(file_name: &str, media_type: &str) -> String {
    let ext = Path::new(file_name)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "heic" | "heif" => "image/heic",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        _ if media_type == "video" => "video/mp4",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use image::{Rgb, RgbImage};
    use tempfile::tempdir;

    fn seed_asset(conn: &Connection, id: &str, path: &Path) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES (?1, ?2, ?3, 'image', 't', 't')",
            params![id, path.display().to_string(), format!("h-{id}")],
        )
        .unwrap();
    }

    #[test]
    fn setup_lock_moveout_roundtrip() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();

        let media = dir.path().join("secret.jpg");
        std::fs::write(&media, b"top secret pixels").unwrap();
        seed_asset(&conn, "a1", &media);

        let vault = dir.path().join("vault");
        let outcome = setup(&conn, "Personal", &vault.to_string_lossy(), "hunter2").unwrap();
        let key = outcome.master_key;
        let vault_id = &outcome.vault_id;

        let res = lock_assets(&conn, vault_id, &key, &["a1".into()]).unwrap();
        assert_eq!(res.locked, 1);
        assert!(!media.exists());
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(locked_count_for(&conn, vault_id).unwrap(), 1);

        // Neither the blob nor the database may contain the plaintext or name.
        let id: String = conn
            .query_row("SELECT id FROM locked_assets", [], |r| r.get(0))
            .unwrap();
        let vault_file: String = conn
            .query_row("SELECT vault_file FROM locked_assets", [], |r| r.get(0))
            .unwrap();
        let blob = std::fs::read(vault.join("blobs").join(&vault_file)).unwrap();
        assert!(!blob.windows(6).any(|w| w == b"secret"));
        let meta_enc: String = conn
            .query_row("SELECT meta_enc FROM locked_assets", [], |r| r.get(0))
            .unwrap();
        assert!(!meta_enc.contains("secret.jpg"));

        assert!(unlock(&conn, vault_id, "wrong").is_err());
        let key2 = unlock(&conn, vault_id, "hunter2").unwrap();
        assert_eq!(key2, key);

        // Names come back only after decryption.
        let listed = list_locked(&conn, vault_id, &key2, None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].file_name, "secret.jpg");

        let out = dir.path().join("restored");
        let mo = move_out(&conn, vault_id, &key2, &[id], &out.to_string_lossy()).unwrap();
        assert_eq!(mo.restored, 1);
        assert_eq!(locked_count_for(&conn, vault_id).unwrap(), 0);
        assert_eq!(
            std::fs::read(out.join("secret.jpg")).unwrap(),
            b"top secret pixels"
        );
    }

    #[test]
    fn recovery_code_resets_the_password() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let outcome = setup(&conn, "Vault", &vault.to_string_lossy(), "original").unwrap();
        let vault_id = &outcome.vault_id;

        // A wrong code is rejected.
        assert!(recover(&conn, vault_id, "ABCD-EFGH-JKMN-PQRS", "newpass").is_err());

        let recovered = recover(&conn, vault_id, &outcome.recovery_code, "newpass").unwrap();
        assert_eq!(recovered, outcome.master_key);

        // Old password no longer works; new one does; code stays valid.
        assert!(unlock(&conn, vault_id, "original").is_err());
        assert_eq!(
            unlock(&conn, vault_id, "newpass").unwrap(),
            outcome.master_key
        );
        assert!(recover(&conn, vault_id, &outcome.recovery_code, "third").is_ok());
    }

    #[test]
    fn existing_vault_can_enable_recovery_once() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let outcome = setup(&conn, "Vault", &vault.to_string_lossy(), "original").unwrap();
        let vault_id = &outcome.vault_id;

        // Simulate a vault created by Phase 1, before recovery columns existed.
        conn.execute(
            "UPDATE vaults
             SET recovery_salt = NULL, recovery_nonce = NULL, recovery_wrapped_key = NULL
             WHERE id = ?1",
            params![vault_id],
        )
        .unwrap();
        assert!(!recovery_configured(&conn, vault_id).unwrap());

        let code = enable_recovery(&conn, vault_id, &outcome.master_key).unwrap();
        assert!(recovery_configured(&conn, vault_id).unwrap());
        assert_eq!(
            recover(&conn, vault_id, &code, "replacement").unwrap(),
            outcome.master_key
        );
        assert!(enable_recovery(&conn, vault_id, &outcome.master_key).is_err());
    }

    #[test]
    fn locking_a_folder_keeps_structure_on_the_way_out() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let outcome = setup(&conn, "Vault", &vault.to_string_lossy(), "hunter2").unwrap();
        let key = outcome.master_key;
        let vault_id = &outcome.vault_id;

        // A folder with a nested subfolder and an unrelated file.
        let source = dir.path().join("Holiday");
        std::fs::create_dir_all(source.join("day-2")).unwrap();
        RgbImage::from_pixel(16, 16, Rgb([9, 9, 9]))
            .save(source.join("beach.png"))
            .unwrap();
        RgbImage::from_pixel(16, 16, Rgb([4, 4, 4]))
            .save(source.join("day-2/sunset.png"))
            .unwrap();
        std::fs::write(source.join("notes.txt"), b"not media").unwrap();

        let result = lock_folder(&conn, vault_id, &key, &source).unwrap();
        assert_eq!(result.locked, 2);
        assert!(result.album_id.is_some());
        assert!(!source.join("beach.png").exists());
        assert!(!source.join("day-2/sunset.png").exists());
        // Non-media is untouched, so the folder is not removed.
        assert!(source.join("notes.txt").exists());

        let albums = list_locked_albums(&conn, vault_id, &key).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].name, "Holiday");
        assert_eq!(albums[0].item_count, 2);

        // Group name is not readable in the database.
        let name_enc: String = conn
            .query_row("SELECT name_enc FROM locked_albums", [], |r| r.get(0))
            .unwrap();
        assert!(!name_enc.contains("Holiday"));

        // Thumbnails were generated in memory for non-indexed files.
        let items = list_locked(&conn, vault_id, &key, Some(&albums[0].id)).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.has_thumb));
        assert!(list_locked(&conn, vault_id, &key, None).unwrap().is_empty());

        let out = dir.path().join("out");
        let mo =
            move_out_album(&conn, vault_id, &key, &albums[0].id, &out.to_string_lossy()).unwrap();
        assert_eq!(mo.restored, 2);
        assert!(out.join("Holiday/beach.png").is_file());
        assert!(out.join("Holiday/day-2/sunset.png").is_file());
        // The now-empty group is pruned.
        assert!(list_locked_albums(&conn, vault_id, &key)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn locking_an_album_moves_it_in_as_a_group() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let outcome = setup(&conn, "Vault", &vault.to_string_lossy(), "hunter2").unwrap();
        let key = outcome.master_key;
        let vault_id = &outcome.vault_id;

        let a = dir.path().join("one.jpg");
        let b = dir.path().join("two.jpg");
        std::fs::write(&a, b"first").unwrap();
        std::fs::write(&b, b"second").unwrap();
        seed_asset(&conn, "a1", &a);
        seed_asset(&conn, "a2", &b);
        conn.execute(
            "INSERT INTO albums (id, name, created_at) VALUES ('alb', 'Private Trip', 't')",
            [],
        )
        .unwrap();
        for id in ["a1", "a2"] {
            conn.execute(
                "INSERT INTO album_assets (album_id, asset_id) VALUES ('alb', ?1)",
                params![id],
            )
            .unwrap();
        }

        let result = lock_album(&conn, vault_id, &key, "alb").unwrap();
        assert_eq!(result.locked, 2);
        assert!(!a.exists() && !b.exists());

        // The library album is gone and the group carries its name.
        let albums_left: i64 = conn
            .query_row("SELECT COUNT(*) FROM albums", [], |r| r.get(0))
            .unwrap();
        assert_eq!(albums_left, 0);
        let locked = list_locked_albums(&conn, vault_id, &key).unwrap();
        assert_eq!(locked[0].name, "Private Trip");
        assert_eq!(locked[0].item_count, 2);
    }

    #[test]
    fn legacy_rows_get_their_metadata_encrypted_on_unlock() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let outcome = setup(&conn, "Vault", &vault.to_string_lossy(), "hunter2").unwrap();
        let vault_id = &outcome.vault_id;

        // Simulate a row written by migration 005 (plaintext columns, no blob).
        conn.execute(
            "INSERT INTO locked_assets
               (id, vault_id, vault_file, thumb_file, meta_enc, locked_at, legacy_file_name,
                legacy_media_type, legacy_size_bytes, legacy_original_path)
             VALUES ('old', ?1, 'old.pv', NULL, NULL, 't', 'holiday.jpg', 'image', 42, '/tmp/holiday.jpg')",
            params![vault_id],
        )
        .unwrap();

        let key = unlock(&conn, vault_id, "hunter2").unwrap();

        let (meta_enc, leftover): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT meta_enc, legacy_file_name FROM locked_assets WHERE id = 'old'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(meta_enc.is_some());
        assert!(leftover.is_none(), "plaintext name should be cleared");

        let items = list_locked(&conn, vault_id, &key, None).unwrap();
        assert_eq!(items[0].file_name, "holiday.jpg");
        assert_eq!(items[0].size_bytes, Some(42));
    }

    #[test]
    fn multiple_vaults_have_independent_keys() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();

        let a = setup(
            &conn,
            "Alpha",
            &dir.path().join("vault-a").to_string_lossy(),
            "pass-a",
        )
        .unwrap();
        let b = setup(
            &conn,
            "Beta",
            &dir.path().join("vault-b").to_string_lossy(),
            "pass-b",
        )
        .unwrap();

        assert_ne!(a.vault_id, b.vault_id);
        assert_ne!(a.master_key, b.master_key);
        assert_ne!(a.recovery_code, b.recovery_code);

        assert!(unlock(&conn, &a.vault_id, "pass-b").is_err());
        assert!(unlock(&conn, &b.vault_id, "pass-a").is_err());
        assert_eq!(unlock(&conn, &a.vault_id, "pass-a").unwrap(), a.master_key);
        assert_eq!(unlock(&conn, &b.vault_id, "pass-b").unwrap(), b.master_key);

        let listed = list_vaults(&conn, Some(&a.vault_id)).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].name, "Alpha");
        assert!(listed[0].unlocked);
        assert!(!listed[1].unlocked);
    }

    /// The vault folder alone — no database — must be enough to get the files
    /// back with their original names and structure. This is what the
    /// `lumora-vault` CLI does on another machine.
    #[test]
    fn vault_folder_alone_restores_names_and_structure() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let vault_str = vault.to_string_lossy().to_string();
        let outcome = setup(&conn, "Vault", &vault_str, "hunter2").unwrap();
        let key = outcome.master_key;
        let vault_id = &outcome.vault_id;

        // One loose file plus a folder locked as a group.
        let loose = dir.path().join("receipt.jpg");
        std::fs::write(&loose, b"loose bytes").unwrap();
        seed_asset(&conn, "a1", &loose);
        lock_assets(&conn, vault_id, &key, &["a1".into()]).unwrap();

        let source = dir.path().join("Holiday");
        std::fs::create_dir_all(source.join("day-2")).unwrap();
        RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))
            .save(source.join("beach.png"))
            .unwrap();
        RgbImage::from_pixel(8, 8, Rgb([4, 5, 6]))
            .save(source.join("day-2/sunset.png"))
            .unwrap();
        lock_folder(&conn, vault_id, &key, &source).unwrap();

        assert!(portable::catalog_exists(&vault_str));

        // Nothing in the vault folder leaks a filename.
        let catalog_raw = std::fs::read(portable::catalog_path(&vault_str)).unwrap();
        assert!(!catalog_raw
            .windows(7)
            .any(|w| w == b"Holiday" || w == b"receipt"));

        let out = dir.path().join("restored");
        let summary =
            portable::unlock_to_dir(&vault_str, &portable::Secret::Password("hunter2"), &out)
                .unwrap();
        assert_eq!(summary.restored, 3);
        assert!(summary.errors.is_empty());
        assert_eq!(
            std::fs::read(out.join("receipt.jpg")).unwrap(),
            b"loose bytes"
        );
        assert!(out.join("Holiday/beach.png").is_file());
        assert!(out.join("Holiday/day-2/sunset.png").is_file());

        // The vault itself is untouched: still locked, still complete.
        assert_eq!(locked_count_for(&conn, vault_id).unwrap(), 3);

        // Wrong password fails; the recovery code works.
        assert!(
            portable::unlock_to_dir(&vault_str, &portable::Secret::Password("nope"), &out).is_err()
        );
        let via_code = dir.path().join("restored-code");
        let summary = portable::unlock_to_dir(
            &vault_str,
            &portable::Secret::RecoveryCode(&outcome.recovery_code),
            &via_code,
        )
        .unwrap();
        assert_eq!(summary.restored, 3);
    }

    /// Deleting from the vault must also shrink the portable catalog, or the
    /// CLI would look for blobs that no longer exist.
    #[test]
    fn catalog_follows_deletions() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let vault_str = vault.to_string_lossy().to_string();
        let outcome = setup(&conn, "Vault", &vault_str, "hunter2").unwrap();
        let key = outcome.master_key;
        let vault_id = &outcome.vault_id;

        let media = dir.path().join("gone.jpg");
        std::fs::write(&media, b"bytes").unwrap();
        seed_asset(&conn, "a1", &media);
        lock_assets(&conn, vault_id, &key, &["a1".into()]).unwrap();

        let id: String = conn
            .query_row("SELECT id FROM locked_assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            portable::read_catalog(&vault_str, &key)
                .unwrap()
                .items
                .len(),
            1
        );

        delete_locked(&conn, vault_id, &key, &[id]).unwrap();
        assert!(portable::read_catalog(&vault_str, &key)
            .unwrap()
            .items
            .is_empty());
    }

    /// Vaults created before the catalog existed get one on the next unlock.
    #[test]
    fn unlock_backfills_a_missing_catalog() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let vault = dir.path().join("vault");
        let vault_str = vault.to_string_lossy().to_string();
        let outcome = setup(&conn, "Vault", &vault_str, "hunter2").unwrap();

        let media = dir.path().join("old.jpg");
        std::fs::write(&media, b"bytes").unwrap();
        seed_asset(&conn, "a1", &media);
        lock_assets(
            &conn,
            &outcome.vault_id,
            &outcome.master_key,
            &["a1".into()],
        )
        .unwrap();

        std::fs::remove_file(portable::catalog_path(&vault_str)).unwrap();
        assert!(!portable::catalog_exists(&vault_str));

        let key = unlock(&conn, &outcome.vault_id, "hunter2").unwrap();
        let catalog = portable::read_catalog(&vault_str, &key).unwrap();
        assert_eq!(catalog.items.len(), 1);
        assert_eq!(catalog.items[0].file_name, "old.jpg");
    }
}
