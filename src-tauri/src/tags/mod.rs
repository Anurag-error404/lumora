//! On-device ImageNet auto-tags via MobileNetV4 (ONNX).
//!
//! Stores top-k labels in `asset_labels` (rebuildable) and refreshes FTS
//! `auto_tags` so search finds them without creating user tags.

pub mod engine;
pub mod worker;

use std::path::PathBuf;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::indexer;
use crate::ml::{self, catalog::ModelKind};

#[derive(Debug, Clone)]
pub struct TagsModelPaths {
    pub model: PathBuf,
    pub labels: PathBuf,
    pub input_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetLabel {
    pub asset_id: String,
    pub label: String,
    pub score: f32,
    pub rank: i32,
    pub model_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagsCoverage {
    pub done: i64,
    pub total: i64,
}

pub fn active_option(app_data: &std::path::Path) -> &'static ml::library::ModelOption {
    let preferred = crate::preferences::load(app_data)
        .map(|p| p.ai.tags_model)
        .unwrap_or_else(|_| "mobilenetv4-small".into());
    if ml::user::is_user_option_id(&preferred) {
        return ml::library::default_option(ml::library::Capability::AutoTags);
    }
    ml::library::resolve_active(ml::library::Capability::AutoTags, &preferred)
}

pub fn active_tags_model_id(app_data: &std::path::Path) -> String {
    crate::preferences::load(app_data)
        .map(|p| p.ai.tags_model)
        .unwrap_or_else(|_| "mobilenetv4-small".into())
}

pub fn active_bundle(app_data: &std::path::Path) -> String {
    let preferred = active_tags_model_id(app_data);
    if ml::user::is_user_option_id(&preferred) {
        return preferred;
    }
    active_option(app_data)
        .bundle
        .unwrap_or(ml::catalog::TAGS_BUNDLE)
        .to_string()
}

pub fn tags_ready(conn: &Connection) -> AppResult<bool> {
    tags_ready_bundle(conn, ml::catalog::TAGS_BUNDLE)
}

pub fn tags_ready_bundle(conn: &Connection, bundle: &str) -> AppResult<bool> {
    for entry in ml::catalog::bundle(bundle) {
        if ml::installed_row(conn, entry.id)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn model_paths_for(
    conn: &Connection,
    bundle: &str,
    input_size: u32,
) -> AppResult<TagsModelPaths> {
    let mut model = None;
    let mut labels = None;
    for entry in ml::catalog::bundle(bundle) {
        if entry.file_name.ends_with(".onnx") {
            model = Some(ml::require_path(conn, entry.id)?);
        } else if entry.file_name.ends_with(".txt") {
            labels = Some(ml::require_path(conn, entry.id)?);
        }
    }
    Ok(TagsModelPaths {
        model: model.ok_or_else(|| {
            crate::error::AppError::msg(format!("auto-tag model missing in bundle {bundle}"))
        })?,
        labels: labels.ok_or_else(|| {
            crate::error::AppError::msg(format!("auto-tag labels missing in bundle {bundle}"))
        })?,
        input_size,
    })
}

/// Replace labels for one asset and refresh FTS.
pub fn store(
    conn: &Connection,
    asset_id: &str,
    labels: &[(String, f32)],
    model_id: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "DELETE FROM asset_labels WHERE asset_id = ?1",
        params![asset_id],
    )?;
    for (rank, (label, score)) in labels.iter().enumerate() {
        conn.execute(
            "INSERT INTO asset_labels (asset_id, label, score, rank, model_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![asset_id, label, *score as f64, rank as i32, model_id, now],
        )?;
    }
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
            ModelKind::Tags.as_str(),
            state,
            error,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

pub fn pending_assets(conn: &Connection, limit: u32) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path
         FROM assets a
         LEFT JOIN (SELECT DISTINCT asset_id FROM asset_labels) l ON l.asset_id = a.id
         LEFT JOIN ml_jobs j
           ON j.asset_id = a.id AND j.kind = ?1
         WHERE a.deleted_at IS NULL
           AND a.media_type = 'image'
           AND l.asset_id IS NULL
           AND (
             j.state IS NULL
             OR j.state = 'done'
             OR (j.state = 'failed' AND j.attempts < 3)
           )
         ORDER BY a.indexed_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![ModelKind::Tags.as_str(), limit], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn coverage(conn: &Connection) -> AppResult<TagsCoverage> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type = 'image'",
        [],
        |r| r.get(0),
    )?;
    let done: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT l.asset_id) FROM asset_labels l
         JOIN assets a ON a.id = l.asset_id AND a.deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(TagsCoverage { done, total })
}

pub fn list_for_asset(conn: &Connection, asset_id: &str) -> AppResult<Vec<AssetLabel>> {
    let mut stmt = conn.prepare(
        "SELECT asset_id, label, score, rank, model_id, created_at
         FROM asset_labels WHERE asset_id = ?1
         ORDER BY rank ASC",
    )?;
    let rows = stmt.query_map(params![asset_id], |r| {
        Ok(AssetLabel {
            asset_id: r.get(0)?,
            label: r.get(1)?,
            score: r.get::<_, f64>(2)? as f32,
            rank: r.get(3)?,
            model_id: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn clear_all(conn: &Connection) -> AppResult<usize> {
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT asset_id FROM asset_labels")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let n = conn.execute("DELETE FROM asset_labels", [])?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE kind = ?1",
        params![ModelKind::Tags.as_str()],
    )?;
    for id in &ids {
        let _ = indexer::refresh_fts(conn, id);
    }
    Ok(n)
}

pub fn invalidate_asset(conn: &Connection, asset_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM asset_labels WHERE asset_id = ?1",
        params![asset_id],
    )?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE asset_id = ?1 AND kind = ?2",
        params![asset_id, ModelKind::Tags.as_str()],
    )?;
    indexer::refresh_fts(conn, asset_id)?;
    Ok(())
}

pub fn display_label(raw: &str) -> String {
    raw.split(',').next().unwrap_or(raw).trim().to_string()
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

    #[test]
    fn store_and_clear_labels() {
        let (_dir, conn) = open();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('a1', '/m/a1.jpg', 'h1', 'image', '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
        store(
            &conn,
            "a1",
            &[("golden retriever".into(), 0.9), ("dog".into(), 0.4)],
            "mobilenetv4-small-in1k",
        )
        .unwrap();
        let labels = list_for_asset(&conn, "a1").unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(coverage(&conn).unwrap().done, 1);
        clear_all(&conn).unwrap();
        assert!(list_for_asset(&conn, "a1").unwrap().is_empty());
    }

    #[test]
    fn display_label_takes_first_synonym() {
        assert_eq!(
            display_label("golden retriever, Labrador"),
            "golden retriever"
        );
    }
}
