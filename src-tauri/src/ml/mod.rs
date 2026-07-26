//! On-device model management.
//!
//! Phase 2 features are additive: with no model installed every Phase 1 code
//! path behaves exactly as before, and the only thing that changes is that
//! semantic search reports itself unavailable.
//!
//! Network policy: downloading is the single approved network operation in the
//! app, and it only ever happens from an explicit user action. Nothing here is
//! called during indexing, browsing, or search.

pub mod catalog;
pub mod clip;
pub mod preprocess;
pub mod vector;

use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use catalog::{CatalogEntry, ModelKind};

/// Read/verify chunk size. Large enough to keep hashing cheap, small enough
/// that a cancelled download does not sit on a huge buffer.
const CHUNK: usize = 1024 * 1024;

/// Refuse anything wildly larger than the catalog claims, so a redirect to an
/// unexpected endpoint cannot fill the disk.
const SIZE_TOLERANCE: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub bundle: String,
    pub kind: String,
    pub version: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub license: String,
    pub installed: bool,
    /// Present once installed.
    pub path: Option<String>,
    pub installed_at: Option<String>,
}

/// What the UI needs to decide between "run semantic search" and "offer to
/// install the model".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MlStatus {
    /// Every file of the semantic bundle is installed and verified.
    pub semantic_ready: bool,
    pub models: Vec<ModelInfo>,
    /// Bytes the semantic bundle would download if not yet installed.
    pub semantic_download_bytes: u64,
    pub installed_bytes: u64,
    pub models_dir: String,
}

pub fn status(conn: &Connection, models_dir: &Path) -> AppResult<MlStatus> {
    let mut models = Vec::new();
    let mut installed_bytes = 0u64;
    for entry in catalog::CATALOG {
        let row = installed_row(conn, entry.id)?;
        if let Some(r) = &row {
            installed_bytes += r.size_bytes.max(0) as u64;
        }
        models.push(ModelInfo {
            id: entry.id.to_string(),
            bundle: entry.bundle.to_string(),
            kind: entry.kind.as_str().to_string(),
            version: entry.version.to_string(),
            file_name: entry.file_name.to_string(),
            size_bytes: entry.size_bytes,
            license: entry.license.to_string(),
            installed: row.is_some(),
            path: row.as_ref().map(|r| r.path.clone()),
            installed_at: row.as_ref().map(|r| r.installed_at.clone()),
        });
    }

    Ok(MlStatus {
        semantic_ready: semantic_ready(conn)?,
        semantic_download_bytes: catalog::bundle_size(catalog::SEMANTIC_BUNDLE),
        installed_bytes,
        models_dir: models_dir.display().to_string(),
        models,
    })
}

/// True only when every file of the semantic bundle is present in the registry.
/// A half-installed bundle is treated as not ready.
pub fn semantic_ready(conn: &Connection) -> AppResult<bool> {
    for entry in catalog::bundle(catalog::SEMANTIC_BUNDLE) {
        if installed_row(conn, entry.id)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct InstalledModel {
    pub path: String,
    pub size_bytes: i64,
    pub installed_at: String,
}

pub fn installed_row(conn: &Connection, id: &str) -> AppResult<Option<InstalledModel>> {
    Ok(conn
        .query_row(
            "SELECT path, size_bytes, installed_at FROM ml_models WHERE id = ?1",
            params![id],
            |r| {
                Ok(InstalledModel {
                    path: r.get(0)?,
                    size_bytes: r.get(1)?,
                    installed_at: r.get(2)?,
                })
            },
        )
        .optional()?)
}

/// Path of an installed model file, or an error explaining how to get it.
pub fn require_path(conn: &Connection, id: &str) -> AppResult<PathBuf> {
    let row = installed_row(conn, id)?.ok_or_else(|| {
        AppError::msg(format!(
            "model '{id}' is not installed — install it from Settings to use this feature"
        ))
    })?;
    let path = PathBuf::from(&row.path);
    if !path.exists() {
        return Err(AppError::msg(format!(
            "model '{id}' is registered but its file is missing; remove and reinstall it"
        )));
    }
    Ok(path)
}

/// Hash a file on disk, streaming so a 350 MB model never lands in memory.
pub fn file_sha256(path: &Path) -> AppResult<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Register a model file that is already on disk, verifying it first.
///
/// Split out from downloading so the verification path is testable without any
/// network, and so a user-supplied file can be adopted the same way.
pub fn register_verified(
    conn: &Connection,
    entry: &CatalogEntry,
    path: &Path,
) -> AppResult<InstalledModel> {
    let actual = file_sha256(path)?;
    if actual != entry.sha256 {
        // Never leave an unverified file where it could later be loaded.
        let _ = std::fs::remove_file(path);
        return Err(AppError::msg(format!(
            "checksum mismatch for '{}': expected {}, got {actual}. The file was discarded.",
            entry.id, entry.sha256
        )));
    }

    let size = std::fs::metadata(path)?.len() as i64;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO ml_models (id, kind, version, path, sha256, size_bytes, dim, installed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(id) DO UPDATE SET
           path = excluded.path,
           sha256 = excluded.sha256,
           size_bytes = excluded.size_bytes,
           dim = excluded.dim,
           installed_at = excluded.installed_at",
        params![
            entry.id,
            entry.kind.as_str(),
            entry.version,
            path.display().to_string(),
            entry.sha256,
            size,
            entry.dim,
            now
        ],
    )?;

    tracing::info!(model = entry.id, bytes = size, "model installed");
    Ok(InstalledModel {
        path: path.display().to_string(),
        size_bytes: size,
        installed_at: now,
    })
}

/// Download one catalog entry, verify it, and register it.
///
/// Downloads to a `.part` file and only moves it into place after the checksum
/// matches, so an interrupted download can never be mistaken for a model.
pub fn download_and_install(
    conn: &Connection,
    models_dir: &Path,
    entry: &CatalogEntry,
    mut on_progress: impl FnMut(u64, u64),
) -> AppResult<InstalledModel> {
    std::fs::create_dir_all(models_dir)?;
    let final_path = models_dir.join(entry.file_name);

    // Already there and still valid? Re-register rather than re-download.
    if final_path.exists() && file_sha256(&final_path)? == entry.sha256 {
        return register_verified(conn, entry, &final_path);
    }

    let part_path = models_dir.join(format!("{}.part", entry.file_name));
    tracing::info!(model = entry.id, url = entry.url, "downloading model");

    let response = ureq::get(entry.url)
        .call()
        .map_err(|e| AppError::msg(format!("download failed for '{}': {e}", entry.id)))?;

    let expected = entry.size_bytes;
    {
        let mut reader = response.into_body().into_reader();
        let mut out = std::fs::File::create(&part_path)?;
        let mut buf = vec![0u8; CHUNK];
        let mut written: u64 = 0;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > expected + SIZE_TOLERANCE {
                let _ = std::fs::remove_file(&part_path);
                return Err(AppError::msg(format!(
                    "download for '{}' exceeded its expected size; aborted",
                    entry.id
                )));
            }
            std::io::Write::write_all(&mut out, &buf[..n])?;
            on_progress(written, expected);
        }
        std::io::Write::flush(&mut out)?;
    }

    std::fs::rename(&part_path, &final_path)?;
    match register_verified(conn, entry, &final_path) {
        Ok(installed) => Ok(installed),
        Err(e) => {
            // register_verified already removed the bad file.
            let _ = std::fs::remove_file(&part_path);
            Err(e)
        }
    }
}

/// Remove a model file and its registry row. Embeddings produced by it are
/// dropped too, because they can no longer be compared against new queries.
pub fn remove(conn: &Connection, models_dir: &Path, id: &str) -> AppResult<()> {
    if let Some(row) = installed_row(conn, id)? {
        let path = PathBuf::from(&row.path);
        // Only ever delete inside our own models directory.
        if path.starts_with(models_dir) && path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    conn.execute("DELETE FROM asset_embeddings WHERE model_id = ?1", params![id])?;
    conn.execute("DELETE FROM ml_models WHERE id = ?1", params![id])?;
    if let Some(entry) = catalog::entry(id) {
        conn.execute(
            "DELETE FROM ml_jobs WHERE kind = ?1",
            params![entry.kind.as_str()],
        )?;
    }
    tracing::info!(model = id, "model removed");
    Ok(())
}

/// Drop every derived vector without touching models or the library.
pub fn clear_embeddings(conn: &Connection) -> AppResult<usize> {
    let n = conn.execute("DELETE FROM asset_embeddings", [])?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE kind IN (?1, ?2)",
        params![
            ModelKind::ClipImage.as_str(),
            ModelKind::ClipText.as_str()
        ],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        (dir, conn)
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn fresh_library_reports_no_models_and_semantic_unavailable() {
        let (dir, conn) = open();
        let st = status(&conn, dir.path()).unwrap();
        assert!(!st.semantic_ready);
        assert!(st.models.iter().all(|m| !m.installed));
        assert_eq!(st.installed_bytes, 0);
        assert!(st.semantic_download_bytes > 0);
    }

    #[test]
    fn require_path_explains_how_to_fix_a_missing_model() {
        let (_dir, conn) = open();
        let err = require_path(&conn, "clip-vit-b32-image").unwrap_err();
        assert!(
            err.to_string().contains("not installed"),
            "unhelpful error: {err}"
        );
    }

    #[test]
    fn registering_rejects_and_deletes_a_file_with_the_wrong_checksum() {
        let (dir, conn) = open();
        let entry = catalog::entry("clip-vit-b32-tokenizer").unwrap();
        let path = dir.path().join(entry.file_name);
        write(&path, b"not the real tokenizer");

        let err = register_verified(&conn, entry, &path).unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"), "{err}");
        assert!(!path.exists(), "unverified file must not be left on disk");
        assert!(installed_row(&conn, entry.id).unwrap().is_none());
    }

    #[test]
    fn registering_accepts_a_file_matching_the_pinned_checksum() {
        let (dir, conn) = open();
        // Pin a synthetic entry to the hash of known bytes so the happy path is
        // testable without downloading a 350 MB model.
        let body = b"lumora test model payload";
        let path = dir.path().join("fake.onnx");
        write(&path, body);
        let digest = file_sha256(&path).unwrap();

        let entry = CatalogEntry {
            id: "test-model",
            bundle: "test",
            kind: ModelKind::ClipImage,
            version: "1",
            file_name: "fake.onnx",
            url: "https://example.invalid/fake.onnx",
            sha256: Box::leak(digest.clone().into_boxed_str()),
            size_bytes: body.len() as u64,
            dim: Some(512),
            license: "MIT",
        };

        let installed = register_verified(&conn, &entry, &path).unwrap();
        assert_eq!(installed.size_bytes, body.len() as i64);
        assert!(installed_row(&conn, "test-model").unwrap().is_some());
        assert_eq!(require_path(&conn, "test-model").unwrap(), path);
    }

    #[test]
    fn removing_a_model_deletes_its_file_and_its_embeddings() {
        let (dir, conn) = open();
        let body = b"payload";
        let path = dir.path().join("m.onnx");
        write(&path, body);
        let digest = file_sha256(&path).unwrap();
        let entry = CatalogEntry {
            id: "test-model",
            bundle: "test",
            kind: ModelKind::ClipImage,
            version: "1",
            file_name: "m.onnx",
            url: "https://example.invalid/m.onnx",
            sha256: Box::leak(digest.into_boxed_str()),
            size_bytes: body.len() as u64,
            dim: Some(512),
            license: "MIT",
        };
        register_verified(&conn, &entry, &path).unwrap();

        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('a1', '/m/a.jpg', 'h1', 'image', '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_embeddings (asset_id, model_id, dim, vector, created_at)
             VALUES ('a1', 'test-model', 2, X'0000', '2026-01-01')",
            [],
        )
        .unwrap();

        remove(&conn, dir.path(), "test-model").unwrap();

        assert!(!path.exists());
        assert!(installed_row(&conn, "test-model").unwrap().is_none());
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM asset_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "embeddings from a removed model are unusable");
    }

    #[test]
    fn remove_never_deletes_files_outside_the_models_directory() {
        let (dir, conn) = open();
        let outside = tempdir().unwrap();
        let victim = outside.path().join("important.onnx");
        write(&victim, b"user data");

        conn.execute(
            "INSERT INTO ml_models (id, kind, version, path, sha256, size_bytes, installed_at)
             VALUES ('rogue', 'clip_image', '1', ?1, 'x', 1, '2026-01-01')",
            params![victim.display().to_string()],
        )
        .unwrap();

        remove(&conn, dir.path(), "rogue").unwrap();

        assert!(victim.exists(), "must not delete outside the models dir");
        assert!(installed_row(&conn, "rogue").unwrap().is_none());
    }

    #[test]
    fn clearing_embeddings_leaves_models_installed() {
        let (dir, conn) = open();
        let body = b"payload";
        let path = dir.path().join("m.onnx");
        write(&path, body);
        let digest = file_sha256(&path).unwrap();
        let entry = CatalogEntry {
            id: "test-model",
            bundle: "test",
            kind: ModelKind::ClipImage,
            version: "1",
            file_name: "m.onnx",
            url: "https://example.invalid/m.onnx",
            sha256: Box::leak(digest.into_boxed_str()),
            size_bytes: body.len() as u64,
            dim: Some(512),
            license: "MIT",
        };
        register_verified(&conn, &entry, &path).unwrap();

        clear_embeddings(&conn).unwrap();

        assert!(installed_row(&conn, "test-model").unwrap().is_some());
        assert!(path.exists());
    }

    #[test]
    fn deleting_an_asset_cascades_to_its_derived_rows() {
        let (_dir, conn) = open();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('a1', '/m/a.jpg', 'h1', 'image', '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_embeddings (asset_id, model_id, dim, vector, created_at)
             VALUES ('a1', 'm', 2, X'0000', '2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ml_jobs (asset_id, kind, state, updated_at)
             VALUES ('a1', 'clip_image', 'done', '2026-01-01')",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM assets WHERE id = 'a1'", []).unwrap();

        let embeddings: i64 = conn
            .query_row("SELECT COUNT(*) FROM asset_embeddings", [], |r| r.get(0))
            .unwrap();
        let jobs: i64 = conn
            .query_row("SELECT COUNT(*) FROM ml_jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!((embeddings, jobs), (0, 0), "derived rows must cascade");
    }
}
