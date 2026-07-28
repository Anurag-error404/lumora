use chrono::{Duration, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::indexer;
use crate::models::AssetSummary;
use crate::search::map_asset;

pub const DEFAULT_RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermanentDeleteResult {
    pub removed_from_library: usize,
    pub files_deleted: usize,
    pub thumbs_deleted: usize,
    pub errors: Vec<String>,
}

pub fn soft_delete(conn: &Connection, asset_ids: &[String]) -> AppResult<usize> {
    let now = Utc::now().to_rfc3339();
    let mut count = 0;
    for id in asset_ids {
        count += conn.execute(
            "UPDATE assets SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id],
        )?;
    }
    Ok(count)
}

pub fn restore(conn: &Connection, asset_ids: &[String]) -> AppResult<usize> {
    let mut count = 0;
    for id in asset_ids {
        count += conn.execute(
            "UPDATE assets SET deleted_at = NULL WHERE id = ?1",
            params![id],
        )?;
        let _ = indexer::refresh_fts(conn, id);
    }
    Ok(count)
}

pub fn list_trash(conn: &Connection, limit: u32, offset: u32) -> AppResult<Vec<AssetSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                created_at, captured_at, indexed_at, favorite, rating, color_label,
                thumbnail_path, camera, lens, deleted_at
         FROM assets
         WHERE deleted_at IS NOT NULL
         ORDER BY deleted_at DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], map_asset)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Permanently remove assets from the library.
/// When `delete_files` is true, also deletes original files from disk (irreversible)
/// and only operates on items already in trash (`deleted_at IS NOT NULL`).
/// When `delete_files` is false, removes library entries for active or trashed assets
/// and leaves original files on disk (used for missing-file cleanup and trash keep-files).
pub fn permanently_delete(
    conn: &Connection,
    asset_ids: &[String],
    delete_files: bool,
) -> AppResult<PermanentDeleteResult> {
    let mut removed = 0usize;
    let mut files_deleted = 0usize;
    let mut thumbs_deleted = 0usize;
    let mut errors = Vec::new();

    for id in asset_ids {
        // Disk deletion is trash-only; library-only removal may target active rows
        // (e.g. missing files still indexed but gone from disk).
        let row: Option<(String, Option<String>)> = if delete_files {
            conn.query_row(
                "SELECT path, thumbnail_path FROM assets
                 WHERE id = ?1 AND deleted_at IS NOT NULL",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()
        } else {
            conn.query_row(
                "SELECT path, thumbnail_path FROM assets WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()
        };

        let Some((path, thumb)) = row else {
            errors.push(format!(
                "{id}: {}",
                if delete_files {
                    "not in trash or missing"
                } else {
                    "missing"
                }
            ));
            continue;
        };

        if delete_files {
            match std::fs::remove_file(&path) {
                Ok(()) => files_deleted += 1,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => errors.push(format!("{path}: {err}")),
            }
        }

        if let Some(thumb_path) = thumb {
            let still_used: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM assets
                     WHERE thumbnail_path = ?1 AND id != ?2",
                    params![thumb_path, id],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            // Thumbnails are keyed by content hash — exact duplicates share one file.
            // Only remove it when nothing else still references it.
            if still_used == 0 {
                match std::fs::remove_file(&thumb_path) {
                    Ok(()) => thumbs_deleted += 1,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => errors.push(format!("thumb {thumb_path}: {err}")),
                }
            }
        }

        conn.execute("DELETE FROM assets_fts WHERE asset_id = ?1", params![id])?;
        removed += conn.execute("DELETE FROM assets WHERE id = ?1", params![id])?;
    }

    if removed == 0 && !errors.is_empty() {
        return Err(AppError::msg(errors.join("; ")));
    }

    Ok(PermanentDeleteResult {
        removed_from_library: removed,
        files_deleted,
        thumbs_deleted,
        errors,
    })
}

/// Permanently empty the entire trash, deleting original files from disk.
pub fn empty_trash(conn: &Connection) -> AppResult<PermanentDeleteResult> {
    let ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    if ids.is_empty() {
        return Ok(PermanentDeleteResult {
            removed_from_library: 0,
            files_deleted: 0,
            thumbs_deleted: 0,
            errors: Vec::new(),
        });
    }
    permanently_delete(conn, &ids, true)
}

/// Purge soft-deleted rows older than retention. Does NOT delete original files.
pub fn purge_expired(conn: &Connection, retention_days: i64) -> AppResult<usize> {
    let cutoff = (Utc::now() - Duration::days(retention_days)).to_rfc3339();
    let ids: Vec<String> = {
        let mut stmt =
            conn.prepare("SELECT id FROM assets WHERE deleted_at IS NOT NULL AND deleted_at < ?1")?;
        let rows = stmt.query_map(params![cutoff], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut purged = 0;
    for id in ids {
        conn.execute("DELETE FROM assets_fts WHERE asset_id = ?1", params![id])?;
        purged += conn.execute("DELETE FROM assets WHERE id = ?1", params![id])?;
    }
    Ok(purged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    #[test]
    fn soft_delete_and_restore() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('x','/x.jpg','h','image','t','t')",
            [],
        )
        .unwrap();
        soft_delete(&conn, &["x".into()]).unwrap();
        let trash = list_trash(&conn, 10, 0).unwrap();
        assert_eq!(trash.len(), 1);
        restore(&conn, &["x".into()]).unwrap();
        let trash = list_trash(&conn, 10, 0).unwrap();
        assert!(trash.is_empty());
    }

    #[test]
    fn permanently_delete_removes_file_when_requested() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("photo.jpg");
        std::fs::write(&media, b"fake").unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        let media_str = media.to_string_lossy().to_string();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, deleted_at)
             VALUES ('x', ?1, 'h', 'image', 't', 't', 't')",
            params![media_str],
        )
        .unwrap();

        let result = permanently_delete(&conn, &["x".into()], true).unwrap();
        assert_eq!(result.removed_from_library, 1);
        assert_eq!(result.files_deleted, 1);
        assert!(!media.exists());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn empty_trash_deletes_all_trashed_files() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.jpg");
        let b = dir.path().join("b.jpg");
        let keep = dir.path().join("keep.jpg");
        std::fs::write(&a, b"a").unwrap();
        std::fs::write(&b, b"b").unwrap();
        std::fs::write(&keep, b"k").unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        for (id, path, deleted) in [
            ("a", a.to_string_lossy().to_string(), true),
            ("b", b.to_string_lossy().to_string(), true),
            ("k", keep.to_string_lossy().to_string(), false),
        ] {
            conn.execute(
                "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, deleted_at)
                 VALUES (?1, ?2, ?3, 'image', 't', 't', ?4)",
                params![
                    id,
                    path,
                    format!("h-{id}"),
                    if deleted {
                        Some("2026-07-26T00:00:00Z")
                    } else {
                        None
                    }
                ],
            )
            .unwrap();
        }

        let result = empty_trash(&conn).unwrap();
        assert_eq!(result.removed_from_library, 2);
        assert_eq!(result.files_deleted, 2);
        assert!(!a.exists());
        assert!(!b.exists());
        assert!(keep.exists());
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1);
        assert!(list_trash(&conn, 10, 0).unwrap().is_empty());
    }

    #[test]
    fn permanently_delete_keeps_shared_thumbnail_for_sibling() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.jpg");
        let b = dir.path().join("b.jpg");
        let thumb = dir.path().join("shared.jpg");
        std::fs::write(&a, b"same").unwrap();
        std::fs::write(&b, b"same").unwrap();
        std::fs::write(&thumb, b"thumb").unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let thumb_str = thumb.to_string_lossy().to_string();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, thumbnail_path, deleted_at)
             VALUES ('a', ?1, 'h', 'image', 't', 't', ?2, 't')",
            params![a.to_string_lossy().to_string(), thumb_str],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, thumbnail_path)
             VALUES ('b', ?1, 'h', 'image', 't', 't', ?2)",
            params![b.to_string_lossy().to_string(), thumb_str],
        )
        .unwrap();

        let result = permanently_delete(&conn, &["a".into()], true).unwrap();
        assert_eq!(result.removed_from_library, 1);
        assert_eq!(result.thumbs_deleted, 0);
        assert!(thumb.exists());
    }

    #[test]
    fn permanently_delete_keep_files_removes_active_missing_entry() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        // Path points at a file that is already gone — typical missing-file case.
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('missing', '/gone/photo.jpg', 'h', 'image', 't', 't')",
            [],
        )
        .unwrap();

        let result = permanently_delete(&conn, &["missing".into()], false).unwrap();
        assert_eq!(result.removed_from_library, 1);
        assert_eq!(result.files_deleted, 0);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn permanently_delete_files_rejects_active_assets() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("photo.jpg");
        std::fs::write(&media, b"fake").unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('active', ?1, 'h', 'image', 't', 't')",
            params![media.to_string_lossy().to_string()],
        )
        .unwrap();

        let err = permanently_delete(&conn, &["active".into()], true).unwrap_err();
        assert!(err.to_string().contains("not in trash"));
        assert!(media.exists());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
