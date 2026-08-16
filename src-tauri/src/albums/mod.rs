use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::Album;

pub fn list_albums(conn: &Connection) -> AppResult<Vec<Album>> {
    let mut stmt = conn.prepare(
        "SELECT
            a.id,
            a.name,
            a.cover_asset_id,
            a.created_at,
            COUNT(x.id),
            COALESCE(
              (
                SELECT c.thumbnail_path
                FROM assets c
                WHERE c.id = a.cover_asset_id AND c.deleted_at IS NULL
              ),
              (
                SELECT x2.thumbnail_path
                FROM album_assets aa2
                JOIN assets x2 ON x2.id = aa2.asset_id AND x2.deleted_at IS NULL
                WHERE aa2.album_id = a.id
                ORDER BY COALESCE(x2.captured_at, x2.indexed_at) DESC
                LIMIT 1
              )
            )
         FROM albums a
         LEFT JOIN album_assets aa ON aa.album_id = a.id
         LEFT JOIN assets x ON x.id = aa.asset_id AND x.deleted_at IS NULL
         GROUP BY a.id
         ORDER BY a.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Album {
            id: r.get(0)?,
            name: r.get(1)?,
            cover_asset_id: r.get(2)?,
            created_at: r.get(3)?,
            asset_count: r.get(4)?,
            cover_thumbnail_path: r.get(5)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Ensure an album has a cover when photos are added and none is set.
pub fn ensure_cover(conn: &Connection, album_id: &str, asset_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE albums
         SET cover_asset_id = ?1
         WHERE id = ?2
           AND (cover_asset_id IS NULL OR cover_asset_id = '')",
        params![asset_id, album_id],
    )?;
    Ok(())
}

/// If the stored cover is no longer in the album, pick another live member (or none).
pub fn sync_cover(conn: &Connection, album_id: &str) -> AppResult<()> {
    let cover: Option<String> = conn.query_row(
        "SELECT cover_asset_id FROM albums WHERE id=?1",
        params![album_id],
        |r| r.get(0),
    )?;
    let cover_ok = match cover.as_deref() {
        None | Some("") => false,
        Some(id) => conn
            .query_row(
                "SELECT 1
                 FROM album_assets aa
                 JOIN assets a ON a.id = aa.asset_id AND a.deleted_at IS NULL
                 WHERE aa.album_id=?1 AND aa.asset_id=?2",
                params![album_id, id],
                |_| Ok(()),
            )
            .is_ok(),
    };
    if cover_ok {
        return Ok(());
    }
    let next: Option<String> = conn
        .query_row(
            "SELECT aa.asset_id
             FROM album_assets aa
             JOIN assets a ON a.id = aa.asset_id AND a.deleted_at IS NULL
             WHERE aa.album_id=?1
             ORDER BY COALESCE(a.captured_at, a.indexed_at) DESC
             LIMIT 1",
            params![album_id],
            |r| r.get(0),
        )
        .ok();
    conn.execute(
        "UPDATE albums SET cover_asset_id=?1 WHERE id=?2",
        params![next, album_id],
    )?;
    Ok(())
}

/// Add an asset to an existing case-insensitive named album, creating it first
/// when needed. The first asset becomes its cover.
pub fn ensure_named_album_with_asset(
    conn: &Connection,
    name: &str,
    asset_id: &str,
) -> AppResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(());
    }
    let album_id: Option<String> = conn
        .query_row(
            "SELECT id FROM albums WHERE name = ?1 COLLATE NOCASE",
            params![name],
            |row| row.get(0),
        )
        .ok();
    let album_id = match album_id {
        Some(id) => id,
        None => {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO albums (id, name, created_at) VALUES (?1, ?2, ?3)",
                params![id, name, chrono::Utc::now().to_rfc3339()],
            )?;
            id
        }
    };
    conn.execute(
        "INSERT OR IGNORE INTO album_assets (album_id, asset_id) VALUES (?1, ?2)",
        params![album_id, asset_id],
    )?;
    ensure_cover(conn, &album_id, asset_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    fn seed_asset(conn: &Connection, id: &str, deleted: bool, thumb: Option<&str>) {
        conn.execute(
            "INSERT INTO assets (
                id, path, hash, media_type, created_at, indexed_at, favorite, rating,
                deleted_at, thumbnail_path
             ) VALUES (?1, ?2, ?3, 'image', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, 0, ?4, ?5)",
            params![
                id,
                format!("/tmp/{id}.jpg"),
                format!("hash-{id}"),
                if deleted {
                    Some("2026-07-26T00:00:00Z")
                } else {
                    None
                },
                thumb,
            ],
        )
        .unwrap();
    }

    #[test]
    fn album_count_excludes_trashed_assets() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO albums (id, name, created_at) VALUES ('alb-1', 'Test', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        seed_asset(&conn, "live", false, Some("/thumbs/live.jpg"));
        seed_asset(&conn, "gone-1", true, Some("/thumbs/gone1.jpg"));
        seed_asset(&conn, "gone-2", true, Some("/thumbs/gone2.jpg"));
        for asset_id in ["live", "gone-1", "gone-2"] {
            conn.execute(
                "INSERT INTO album_assets (album_id, asset_id) VALUES ('alb-1', ?1)",
                params![asset_id],
            )
            .unwrap();
        }

        let albums = list_albums(&conn).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].name, "Test");
        assert_eq!(albums[0].asset_count, 1);
        assert_eq!(
            albums[0].cover_thumbnail_path.as_deref(),
            Some("/thumbs/live.jpg")
        );
    }

    #[test]
    fn prefers_explicit_cover_thumbnail() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "first", false, Some("/thumbs/first.jpg"));
        seed_asset(&conn, "cover", false, Some("/thumbs/cover.jpg"));
        conn.execute(
            "INSERT INTO albums (id, name, cover_asset_id, created_at)
             VALUES ('alb-1', 'Covered', 'cover', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        for asset_id in ["first", "cover"] {
            conn.execute(
                "INSERT INTO album_assets (album_id, asset_id) VALUES ('alb-1', ?1)",
                params![asset_id],
            )
            .unwrap();
        }
        let albums = list_albums(&conn).unwrap();
        assert_eq!(
            albums[0].cover_thumbnail_path.as_deref(),
            Some("/thumbs/cover.jpg")
        );
    }

    #[test]
    fn sync_cover_replaces_cover_removed_from_album() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "keep", false, Some("/thumbs/keep.jpg"));
        seed_asset(&conn, "cover", false, Some("/thumbs/cover.jpg"));
        conn.execute(
            "INSERT INTO albums (id, name, cover_asset_id, created_at)
             VALUES ('alb-1', 'Covered', 'cover', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        for asset_id in ["keep", "cover"] {
            conn.execute(
                "INSERT INTO album_assets (album_id, asset_id) VALUES ('alb-1', ?1)",
                params![asset_id],
            )
            .unwrap();
        }
        conn.execute(
            "DELETE FROM album_assets WHERE album_id='alb-1' AND asset_id='cover'",
            [],
        )
        .unwrap();
        sync_cover(&conn, "alb-1").unwrap();
        let cover: String = conn
            .query_row(
                "SELECT cover_asset_id FROM albums WHERE id='alb-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cover, "keep");
    }
}
