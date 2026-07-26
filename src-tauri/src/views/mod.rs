use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::models::AssetSummary;
use crate::search;

pub fn record_view(conn: &Connection, asset_id: &str) -> AppResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets WHERE id = ?1 AND deleted_at IS NULL)",
        params![asset_id],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO asset_views (asset_id, viewed_at) VALUES (?1, ?2)
         ON CONFLICT(asset_id) DO UPDATE SET viewed_at = excluded.viewed_at",
        params![asset_id, now],
    )?;
    Ok(())
}

pub fn list_recently_viewed(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width, a.height,
                a.duration_ms, a.created_at, a.captured_at, a.indexed_at, a.favorite, a.rating,
                a.color_label, a.thumbnail_path, a.camera, a.lens, a.deleted_at
         FROM asset_views v
         JOIN assets a ON a.id = v.asset_id
         WHERE a.deleted_at IS NULL
         ORDER BY v.viewed_at DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], search::map_asset)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
