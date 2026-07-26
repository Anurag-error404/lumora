//! Everything needed to open a vault folder **without** the app database.
//!
//! A vault directory is self-contained:
//!
//! ```text
//! <vault>/
//!   vault.json    → wrapped master key + KDF parameters (no secrets)
//!   catalog.pv    → encrypted list of items and group names
//!   blobs/*.pv    → encrypted file contents
//!   blobs/*.pt    → encrypted thumbnails
//! ```
//!
//! The app writes `catalog.pv` after every change so the same folder can be
//! carried to another machine and decrypted by the `lumora-vault` CLI.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

use super::crypto::{self, KdfParams, MASTER_KEY_LEN};

pub const BLOBS_DIR: &str = "blobs";
pub const MANIFEST_FILE: &str = "vault.json";
pub const CATALOG_FILE: &str = "catalog.pv";
pub const CATALOG_VERSION: u32 = 1;

/// Portable manifest written into the vault folder so a vault can be recognised
/// independently of the app database. Contains only wrapped-key material.
#[derive(Serialize, Deserialize)]
pub struct VaultManifest {
    pub version: u32,
    pub salt: String,
    pub wrap_nonce: String,
    pub wrapped_key: String,
    pub kdf_m_cost: u32,
    pub kdf_t_cost: u32,
    pub kdf_p_cost: u32,
    pub recovery_salt: Option<String>,
    pub recovery_nonce: Option<String>,
    pub recovery_wrapped_key: Option<String>,
    pub created_at: String,
}

/// A locked group (album or folder) as stored in the portable catalog.
#[derive(Serialize, Deserialize, Clone)]
pub struct CatalogAlbum {
    pub id: String,
    pub name: String,
}

/// One locked item, with just enough to restore it to its original name/path.
#[derive(Serialize, Deserialize, Clone)]
pub struct CatalogItem {
    pub id: String,
    pub vault_file: String,
    pub album_id: Option<String>,
    pub file_name: String,
    /// Path relative to the group root; equals `file_name` for loose items.
    pub rel_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub albums: Vec<CatalogAlbum>,
    pub items: Vec<CatalogItem>,
}

/// How the caller proves ownership of the vault.
pub enum Secret<'a> {
    Password(&'a str),
    RecoveryCode(&'a str),
}

pub struct RestoreSummary {
    pub restored: usize,
    pub errors: Vec<String>,
}

pub fn blobs_root(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(BLOBS_DIR)
}

pub fn manifest_path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(MANIFEST_FILE)
}

pub fn catalog_path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(CATALOG_FILE)
}

pub fn write_manifest(vault_path: &str, manifest: &VaultManifest) {
    if let Ok(json) = serde_json::to_string_pretty(manifest) {
        let _ = std::fs::write(manifest_path(vault_path), json);
    }
}

pub fn read_manifest(vault_path: &str) -> AppResult<VaultManifest> {
    let path = manifest_path(vault_path);
    let raw = std::fs::read(&path).map_err(|e| {
        AppError::msg(format!(
            "no vault found at {} ({e}) — expected a {MANIFEST_FILE} file",
            vault_path
        ))
    })?;
    serde_json::from_slice(&raw).map_err(|e| AppError::msg(format!("corrupt {MANIFEST_FILE}: {e}")))
}

fn decode_hex(value: &str, what: &str) -> AppResult<Vec<u8>> {
    hex::decode(value).map_err(|_| AppError::msg(format!("corrupt vault manifest: {what}")))
}

/// Re-derive the master key from the manifest using a password or recovery code.
pub fn unwrap_key(
    manifest: &VaultManifest,
    secret: &Secret<'_>,
) -> AppResult<[u8; MASTER_KEY_LEN]> {
    let kdf = KdfParams {
        m_cost: manifest.kdf_m_cost,
        t_cost: manifest.kdf_t_cost,
        p_cost: manifest.kdf_p_cost,
    };

    match secret {
        Secret::Password(password) => crypto::unwrap_master_key(
            password,
            &decode_hex(&manifest.salt, "salt")?,
            kdf,
            &decode_hex(&manifest.wrap_nonce, "nonce")?,
            &decode_hex(&manifest.wrapped_key, "key")?,
        ),
        Secret::RecoveryCode(code) => {
            let (salt, nonce, wrapped) = match (
                manifest.recovery_salt.as_ref(),
                manifest.recovery_nonce.as_ref(),
                manifest.recovery_wrapped_key.as_ref(),
            ) {
                (Some(s), Some(n), Some(w)) => (s, n, w),
                _ => return Err(AppError::msg("this vault has no recovery code")),
            };
            crypto::unwrap_master_key(
                &crypto::normalize_recovery_code(code),
                &decode_hex(salt, "recovery salt")?,
                kdf,
                &decode_hex(nonce, "recovery nonce")?,
                &decode_hex(wrapped, "recovery key")?,
            )
            .map_err(|_| AppError::msg("incorrect recovery code"))
        }
    }
}

pub fn write_catalog(
    vault_path: &str,
    key: &[u8; MASTER_KEY_LEN],
    catalog: &Catalog,
) -> AppResult<()> {
    let json = serde_json::to_vec(catalog).map_err(|e| AppError::msg(e.to_string()))?;
    let sealed = crypto::encrypt_blob(key, &json)?;
    std::fs::write(catalog_path(vault_path), sealed)?;
    Ok(())
}

pub fn catalog_exists(vault_path: &str) -> bool {
    catalog_path(vault_path).is_file()
}

pub fn read_catalog(vault_path: &str, key: &[u8; MASTER_KEY_LEN]) -> AppResult<Catalog> {
    let raw = std::fs::read(catalog_path(vault_path)).map_err(|_| {
        AppError::msg(
            "this vault has no portable catalog yet — open it once in LUMORA to upgrade it",
        )
    })?;
    let json = crypto::decrypt_blob(key, &raw)?;
    serde_json::from_slice(&json).map_err(|e| AppError::msg(format!("corrupt catalog: {e}")))
}

/// Decrypt every item in the vault into `out_dir`, restoring group folders and
/// relative paths. The vault itself is left untouched.
pub fn unlock_to_dir(
    vault_path: &str,
    secret: &Secret<'_>,
    out_dir: &Path,
) -> AppResult<RestoreSummary> {
    let manifest = read_manifest(vault_path)?;
    let key = unwrap_key(&manifest, secret)?;
    let catalog = read_catalog(vault_path, &key)?;

    std::fs::create_dir_all(out_dir)?;
    let blobs = blobs_root(vault_path);

    let mut restored = 0usize;
    let mut errors = Vec::new();

    for item in &catalog.items {
        let blob = match std::fs::read(blobs.join(&item.vault_file)) {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!("{}: missing vault file ({e})", item.file_name));
                continue;
            }
        };
        let plaintext = match crypto::decrypt_blob(&key, &blob) {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!("{}: {e}", item.file_name));
                continue;
            }
        };

        let mut target_dir = out_dir.to_path_buf();
        if let Some(album_id) = &item.album_id {
            let name = catalog
                .albums
                .iter()
                .find(|a| &a.id == album_id)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Locked".to_string());
            target_dir = target_dir.join(sanitize_component(&name));
        }

        let rel = if item.rel_path.is_empty() {
            item.file_name.clone()
        } else {
            item.rel_path.clone()
        };
        if let Some(parent) = Path::new(&rel).parent() {
            if !parent.as_os_str().is_empty() {
                target_dir = target_dir.join(parent);
            }
        }
        if let Err(e) = std::fs::create_dir_all(&target_dir) {
            errors.push(format!("{}: {e}", item.file_name));
            continue;
        }

        let leaf = Path::new(&rel)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| item.file_name.clone());
        let target = unique_path(&target_dir, &leaf);
        if let Err(e) = std::fs::write(&target, &plaintext) {
            errors.push(format!("{}: {e}", item.file_name));
            continue;
        }
        restored += 1;
    }

    Ok(RestoreSummary { restored, errors })
}

/// Keep a decrypted group name safe to use as a single directory name.
pub fn sanitize_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "Locked".to_string()
    } else {
        trimmed
    }
}

/// Build a non-colliding path within `dir` for `file_name`.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let ext = Path::new(file_name)
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..10_000 {
        let candidate = dir.join(format!("{stem}-{n}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}{ext}", Uuid::new_v4()))
}
