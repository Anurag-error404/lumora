//! On-device OCR: RapidOCR PP-OCRv4 via ONNX Runtime.
//!
//! Detects text boxes, recognizes each line, stores the joined text in
//! `asset_text`, and refreshes FTS so plain search finds OCR words.

pub mod engine;
pub mod worker;

use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::indexer;
use crate::ml::{self, catalog::ModelKind};

#[derive(Debug, Clone)]
pub struct OcrModelPaths {
    pub det: PathBuf,
    pub rec: PathBuf,
    pub dict: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetText {
    pub asset_id: String,
    pub text: String,
    pub lang: Option<String>,
    pub confidence: f32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrCoverage {
    pub done: i64,
    pub total: i64,
}

pub fn active_bundle(app_data: &std::path::Path) -> String {
    let preferred = crate::preferences::load(app_data)
        .map(|p| p.ai.ocr_model)
        .unwrap_or_else(|_| "rapidocr-ppv4".into());
    let opt = ml::library::resolve_active(ml::library::Capability::Ocr, &preferred);
    opt.bundle
        .unwrap_or(ml::catalog::OCR_BUNDLE)
        .to_string()
}

/// True when every OCR bundle file is registered.
pub fn ocr_ready(conn: &Connection) -> AppResult<bool> {
    ocr_ready_bundle(conn, ml::catalog::OCR_BUNDLE)
}

pub fn ocr_ready_bundle(conn: &Connection, bundle: &str) -> AppResult<bool> {
    for entry in ml::catalog::bundle(bundle) {
        if ml::installed_row(conn, entry.id)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn model_paths_for(conn: &Connection, bundle: &str) -> AppResult<OcrModelPaths> {
    let mut det = None;
    let mut rec = None;
    let mut dict = None;
    for entry in ml::catalog::bundle(bundle) {
        match entry.kind {
            ModelKind::OcrDetect => det = Some(ml::require_path(conn, entry.id)?),
            ModelKind::OcrRecognize if entry.file_name.ends_with(".txt") => {
                dict = Some(ml::require_path(conn, entry.id)?)
            }
            ModelKind::OcrRecognize => rec = Some(ml::require_path(conn, entry.id)?),
            _ => {}
        }
    }
    Ok(OcrModelPaths {
        det: det.ok_or_else(|| {
            crate::error::AppError::msg(format!("OCR detect model missing in bundle {bundle}"))
        })?,
        rec: rec.ok_or_else(|| {
            crate::error::AppError::msg(format!("OCR recognize model missing in bundle {bundle}"))
        })?,
        dict: dict.ok_or_else(|| {
            crate::error::AppError::msg(format!("OCR dictionary missing in bundle {bundle}"))
        })?,
    })
}

/// Upsert OCR text and refresh FTS so search can see it.
pub fn store(
    conn: &Connection,
    asset_id: &str,
    text: &str,
    confidence: f32,
    lang: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO asset_text (asset_id, text, lang, confidence, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(asset_id) DO UPDATE SET
           text = excluded.text,
           lang = excluded.lang,
           confidence = excluded.confidence,
           created_at = excluded.created_at",
        params![asset_id, text, lang, confidence as f64, now],
    )?;
    mark_job(conn, asset_id, "done", None)?;
    indexer::refresh_fts(conn, asset_id)?;
    Ok(())
}

pub fn mark_job(
    conn: &Connection,
    asset_id: &str,
    state: &str,
    error: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO ml_jobs (asset_id, kind, state, attempts, error, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5)
         ON CONFLICT(asset_id, kind) DO UPDATE SET
           state = excluded.state,
           attempts = ml_jobs.attempts + 1,
           error = excluded.error,
           updated_at = excluded.updated_at",
        params![
            asset_id,
            ModelKind::Ocr.as_str(),
            state,
            error,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Images that still need OCR.
pub fn pending_assets(conn: &Connection, limit: u32) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path
         FROM assets a
         LEFT JOIN asset_text t ON t.asset_id = a.id
         LEFT JOIN ml_jobs j
           ON j.asset_id = a.id AND j.kind = ?1
         WHERE a.deleted_at IS NULL
           AND a.media_type = 'image'
           AND t.asset_id IS NULL
           AND (j.state IS NULL OR (j.state = 'failed' AND j.attempts < 3))
         ORDER BY a.indexed_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![ModelKind::Ocr.as_str(), limit], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn coverage(conn: &Connection) -> AppResult<OcrCoverage> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type = 'image'",
        [],
        |r| r.get(0),
    )?;
    let done: i64 = conn.query_row(
        "SELECT COUNT(*) FROM asset_text t
         JOIN assets a ON a.id = t.asset_id AND a.deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(OcrCoverage { done, total })
}

pub fn get_asset_text(conn: &Connection, asset_id: &str) -> AppResult<Option<AssetText>> {
    Ok(conn
        .query_row(
            "SELECT asset_id, text, lang, confidence, created_at
             FROM asset_text WHERE asset_id = ?1",
            params![asset_id],
            |r| {
                Ok(AssetText {
                    asset_id: r.get(0)?,
                    text: r.get(1)?,
                    lang: r.get(2)?,
                    confidence: r.get::<_, f64>(3)? as f32,
                    created_at: r.get(4)?,
                })
            },
        )
        .optional()?)
}

/// Drop all OCR text and related jobs; refresh FTS for affected assets.
pub fn clear_all(conn: &Connection) -> AppResult<usize> {
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT asset_id FROM asset_text")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let n = conn.execute("DELETE FROM asset_text", [])?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE kind = ?1",
        params![ModelKind::Ocr.as_str()],
    )?;
    for id in &ids {
        let _ = indexer::refresh_fts(conn, id);
    }
    Ok(n)
}

/// Invalidate OCR for one asset (e.g. after an image edit).
pub fn invalidate_asset(conn: &Connection, asset_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM asset_text WHERE asset_id = ?1",
        params![asset_id],
    )?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE asset_id = ?1 AND kind = ?2",
        params![asset_id, ModelKind::Ocr.as_str()],
    )?;
    indexer::refresh_fts(conn, asset_id)?;
    Ok(())
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::search;
    use tempfile::tempdir;

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        (dir, conn)
    }

    fn add_image(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES (?1, ?2, ?3, 'image', '2026-01-01', '2026-01-01')",
            params![id, format!("/m/{id}.jpg"), format!("h-{id}")],
        )
        .unwrap();
    }

    #[test]
    fn store_makes_ocr_text_searchable_via_fts() {
        let (_dir, conn) = open();
        add_image(&conn, "a1");
        // Seed empty FTS row so refresh has a filename baseline.
        crate::indexer::refresh_fts(&conn, "a1").unwrap();
        store(&conn, "a1", "invoice total amount due", 0.9, Some("en")).unwrap();

        let rows = search::search_assets(&conn, "invoice", 10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "a1");
    }

    #[test]
    fn deleting_asset_cascades_asset_text() {
        let (_dir, conn) = open();
        add_image(&conn, "a1");
        store(&conn, "a1", "hello", 0.5, None).unwrap();
        conn.execute("DELETE FROM assets WHERE id = 'a1'", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM asset_text", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn pending_respects_three_failure_cap() {
        let (_dir, conn) = open();
        add_image(&conn, "broken");
        for _ in 0..3 {
            assert_eq!(pending_assets(&conn, 50).unwrap().len(), 1);
            mark_job(&conn, "broken", "failed", Some("boom")).unwrap();
        }
        assert!(pending_assets(&conn, 50).unwrap().is_empty());
    }

    #[test]
    fn invalidate_clears_text_and_requeues() {
        let (_dir, conn) = open();
        add_image(&conn, "a1");
        store(&conn, "a1", "old text", 0.8, None).unwrap();
        invalidate_asset(&conn, "a1").unwrap();
        assert!(get_asset_text(&conn, "a1").unwrap().is_none());
        assert_eq!(pending_assets(&conn, 50).unwrap()[0].0, "a1");
    }
}
