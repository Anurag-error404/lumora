//! On-device Florence-2 image captions.

pub mod engine;
pub mod worker;

use std::path::PathBuf;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::indexer;
use crate::ml::{self, catalog::ModelKind};

#[derive(Debug, Clone)]
pub struct CaptionsModelPaths {
    pub vision: PathBuf,
    pub embed: PathBuf,
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub tokenizer: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetCaption {
    pub asset_id: String,
    pub caption: String,
    pub model_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionsCoverage {
    pub done: i64,
    pub total: i64,
}

pub fn active_option(app_data: &std::path::Path) -> &'static ml::library::ModelOption {
    let preferred = crate::preferences::load(app_data)
        .map(|p| p.ai.captions_model)
        .unwrap_or_else(|_| "florence-2-base-ft".into());
    ml::library::resolve_active(ml::library::Capability::Captions, &preferred)
}

pub fn active_bundle(app_data: &std::path::Path) -> String {
    active_option(app_data)
        .bundle
        .unwrap_or(ml::catalog::CAPTIONS_BUNDLE)
        .to_string()
}

pub fn captions_ready(conn: &Connection) -> AppResult<bool> {
    captions_ready_bundle(conn, ml::catalog::CAPTIONS_BUNDLE)
}

pub fn captions_ready_bundle(conn: &Connection, bundle: &str) -> AppResult<bool> {
    ml::catalog::bundle(bundle)
        .try_fold(true, |_, entry| Ok(ml::installed_row(conn, entry.id)?.is_some()))
}

pub fn model_paths_for(conn: &Connection, bundle: &str) -> AppResult<CaptionsModelPaths> {
    let mut vision = None;
    let mut embed = None;
    let mut encoder = None;
    let mut decoder = None;
    let mut tokenizer = None;
    for entry in ml::catalog::bundle(bundle) {
        let path = ml::require_path(conn, entry.id)?;
        match entry.kind {
            ModelKind::CaptionVision => vision = Some(path),
            ModelKind::CaptionEmbed => embed = Some(path),
            ModelKind::CaptionEncoder => encoder = Some(path),
            ModelKind::CaptionDecoder => decoder = Some(path),
            ModelKind::CaptionTokenizer => tokenizer = Some(path),
            _ => {}
        }
    }
    let missing = |name| AppError::msg(format!("Florence-2 {name} missing in bundle {bundle}"));
    Ok(CaptionsModelPaths {
        vision: vision.ok_or_else(|| missing("vision model"))?,
        embed: embed.ok_or_else(|| missing("token embeddings"))?,
        encoder: encoder.ok_or_else(|| missing("encoder"))?,
        decoder: decoder.ok_or_else(|| missing("decoder"))?,
        tokenizer: tokenizer.ok_or_else(|| missing("tokenizer"))?,
    })
}

pub fn store(conn: &Connection, asset_id: &str, caption: &str, model_id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO asset_captions (asset_id, caption, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(asset_id) DO UPDATE SET caption=excluded.caption, model_id=excluded.model_id,
         created_at=excluded.created_at",
        params![asset_id, caption, model_id, now],
    )?;
    mark_job(conn, asset_id, "done", None)?;
    indexer::refresh_fts(conn, asset_id)
}

pub fn mark_job(conn: &Connection, asset_id: &str, state: &str, error: Option<&str>) -> AppResult<()> {
    conn.execute(
        "INSERT INTO ml_jobs (asset_id, kind, state, attempts, error, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5)
         ON CONFLICT(asset_id, kind) DO UPDATE SET state=excluded.state, attempts=ml_jobs.attempts+1,
         error=excluded.error, updated_at=excluded.updated_at",
        params![asset_id, ModelKind::Captions.as_str(), state, error, chrono::Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn pending_assets(conn: &Connection, limit: u32) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path FROM assets a
         LEFT JOIN asset_captions c ON c.asset_id=a.id
         LEFT JOIN ml_jobs j ON j.asset_id=a.id AND j.kind=?1
         WHERE a.deleted_at IS NULL AND a.media_type='image' AND c.asset_id IS NULL
           AND (j.state IS NULL OR j.state='done' OR (j.state='failed' AND j.attempts < 3))
         ORDER BY a.indexed_at DESC LIMIT ?2",
    )?;
    let assets = stmt
        .query_map(params![ModelKind::Captions.as_str(), limit], |r| Ok((r.get(0)?, r.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(assets)
}

pub fn coverage(conn: &Connection) -> AppResult<CaptionsCoverage> {
    let total = conn.query_row("SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type='image'", [], |r| r.get(0))?;
    let done = conn.query_row(
        "SELECT COUNT(*) FROM asset_captions c JOIN assets a ON a.id=c.asset_id AND a.deleted_at IS NULL",
        [], |r| r.get(0),
    )?;
    Ok(CaptionsCoverage { done, total })
}

pub fn get_for_asset(conn: &Connection, asset_id: &str) -> AppResult<Option<AssetCaption>> {
    Ok(conn.query_row(
        "SELECT asset_id, caption, model_id, created_at FROM asset_captions WHERE asset_id=?1",
        params![asset_id],
        |r| Ok(AssetCaption { asset_id: r.get(0)?, caption: r.get(1)?, model_id: r.get(2)?, created_at: r.get(3)? }),
    ).optional()?)
}

pub fn clear_all(conn: &Connection) -> AppResult<usize> {
    let ids: Vec<String> = conn.prepare("SELECT asset_id FROM asset_captions")?
        .query_map([], |r| r.get(0))?.filter_map(|r| r.ok()).collect();
    let n = conn.execute("DELETE FROM asset_captions", [])?;
    conn.execute("DELETE FROM ml_jobs WHERE kind=?1", params![ModelKind::Captions.as_str()])?;
    for id in ids { let _ = indexer::refresh_fts(conn, &id); }
    Ok(n)
}

pub fn invalidate_asset(conn: &Connection, asset_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM asset_captions WHERE asset_id=?1", params![asset_id])?;
    conn.execute("DELETE FROM ml_jobs WHERE asset_id=?1 AND kind=?2", params![asset_id, ModelKind::Captions.as_str()])?;
    indexer::refresh_fts(conn, asset_id)
}
