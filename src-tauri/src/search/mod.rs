pub mod filters;

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::models::{AssetSummary, FacetCount, TagBrowseFilter};
use crate::search::filters::{parse_query, ParsedQuery};

pub fn list_assets(
    conn: &Connection,
    limit: u32,
    offset: u32,
    include_deleted: bool,
) -> AppResult<Vec<AssetSummary>> {
    let sql = if include_deleted {
        "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                created_at, captured_at, indexed_at, favorite, rating, color_label,
                thumbnail_path, camera, lens, deleted_at
         FROM assets
         ORDER BY COALESCE(captured_at, created_at) DESC
         LIMIT ?1 OFFSET ?2"
    } else {
        "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                created_at, captured_at, indexed_at, favorite, rating, color_label,
                thumbnail_path, camera, lens, deleted_at
         FROM assets
         WHERE deleted_at IS NULL
         ORDER BY COALESCE(captured_at, created_at) DESC
         LIMIT ?1 OFFSET ?2"
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![limit, offset], map_asset)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn search_assets(
    conn: &Connection,
    raw: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let parsed = parse_query(raw);
    if parsed.is_empty_browse() {
        return list_assets(conn, limit, offset, false);
    }
    search_assets_impl(conn, &parsed, limit, offset)
}

pub fn list_assets_for_tag(
    conn: &Connection,
    tag_id: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width, a.height,
                a.duration_ms, a.created_at, a.captured_at, a.indexed_at, a.favorite,
                a.rating, a.color_label, a.thumbnail_path, a.camera, a.lens, a.deleted_at
         FROM assets a
         JOIN asset_tags at ON at.asset_id = a.id
         WHERE at.tag_id = ?1 AND a.deleted_at IS NULL
         ORDER BY COALESCE(a.captured_at, a.created_at) DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(params![tag_id, limit, offset], map_asset)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

/// Browse assets matching any combination of tags, ratings, and colour labels.
/// - Within each facet list: OR (e.g. rating 4 OR 5)
/// - Across facet groups: AND (e.g. has selected tag AND rating AND colour)
/// - Multiple tags: asset must have ALL selected tags
pub fn list_assets_for_browse_filter(
    conn: &Connection,
    filter: &TagBrowseFilter,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    if filter.is_empty() {
        return Ok(Vec::new());
    }

    let mut wheres = vec!["a.deleted_at IS NULL".to_string()];
    let mut values: Vec<String> = Vec::new();

    if !filter.tag_ids.is_empty() {
        let placeholders = filter
            .tag_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        let n = filter.tag_ids.len();
        wheres.push(format!(
            "a.id IN (
                SELECT asset_id FROM asset_tags
                WHERE tag_id IN ({placeholders})
                GROUP BY asset_id
                HAVING COUNT(DISTINCT tag_id) = {n}
            )"
        ));
        values.extend(filter.tag_ids.iter().cloned());
    }

    if !filter.ratings.is_empty() {
        let placeholders = filter
            .ratings
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        wheres.push(format!("a.rating IN ({placeholders})"));
        values.extend(filter.ratings.iter().map(|r| r.to_string()));
    }

    if !filter.color_labels.is_empty() {
        let placeholders = filter
            .color_labels
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");
        wheres.push(format!("a.color_label IN ({placeholders})"));
        values.extend(filter.color_labels.iter().cloned());
    }

    let sql = format!(
        "SELECT a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width, a.height,
                a.duration_ms, a.created_at, a.captured_at, a.indexed_at, a.favorite,
                a.rating, a.color_label, a.thumbnail_path, a.camera, a.lens, a.deleted_at
         FROM assets a
         WHERE {where_clause}
         ORDER BY COALESCE(a.captured_at, a.created_at) DESC
         LIMIT ? OFFSET ?",
        where_clause = wheres.join(" AND ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut param_refs: Vec<&dyn rusqlite::types::ToSql> = values
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    let limit_s = limit.to_string();
    let offset_s = offset.to_string();
    param_refs.push(&limit_s);
    param_refs.push(&offset_s);

    let rows = stmt.query_map(param_refs.as_slice(), map_asset)?;
    Ok(rows.filter_map(|row| row.ok()).collect())
}

/// Counts of live assets grouped by rating (1-5) and by colour label.
pub fn facet_counts(conn: &Connection) -> AppResult<(Vec<FacetCount>, Vec<FacetCount>)> {
    let mut rating_stmt = conn.prepare(
        "SELECT rating, COUNT(*)
         FROM assets
         WHERE deleted_at IS NULL AND rating > 0
         GROUP BY rating
         ORDER BY rating DESC",
    )?;
    let ratings = rating_stmt
        .query_map([], |r| {
            Ok(FacetCount {
                value: r.get::<_, i64>(0)?.to_string(),
                count: r.get(1)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut label_stmt = conn.prepare(
        "SELECT color_label, COUNT(*)
         FROM assets
         WHERE deleted_at IS NULL AND color_label IS NOT NULL AND color_label != ''
         GROUP BY color_label
         ORDER BY COUNT(*) DESC",
    )?;
    let labels = label_stmt
        .query_map([], |r| {
            Ok(FacetCount {
                value: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok((ratings, labels))
}

fn search_assets_impl(
    conn: &Connection,
    parsed: &ParsedQuery,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let mut joins = String::new();
    let mut wheres = vec!["a.deleted_at IS NULL".to_string()];
    let mut values: Vec<String> = Vec::new();
    let mut order_by = "COALESCE(a.captured_at, a.created_at) DESC".to_string();

    if let Some(text) = &parsed.text {
        // Subquery keeps MATCH on the FTS table name (required by SQLite) while
        // still exposing bm25 rank. OCR text is weighted highest so a keyword
        // printed in a photo outranks a weak filename coincidence.
        // Weights: filename, tags, camera, lens, ocr_text, people, auto_tags
        joins.push_str(
            " JOIN (
                SELECT rowid, asset_id,
                       bm25(assets_fts, 5.0, 4.0, 1.0, 1.0, 12.0, 6.0, 3.0) AS rank
                FROM assets_fts
                WHERE assets_fts MATCH ?
              ) f ON f.asset_id = a.id",
        );
        values.push(format_fts_query(text));
        order_by = "f.rank, COALESCE(a.captured_at, a.created_at) DESC".to_string();
    }

    if let Some(camera) = &parsed.camera {
        wheres.push("LOWER(COALESCE(a.camera,'')) LIKE ?".to_string());
        values.push(format!("%{}%", camera.to_lowercase()));
    }
    if let Some(min_rating) = parsed.min_rating {
        wheres.push("a.rating >= ?".to_string());
        values.push(min_rating.to_string());
    }
    if let Some(before) = &parsed.before {
        wheres.push("COALESCE(a.captured_at, a.created_at) < ?".to_string());
        values.push(before.clone());
    }
    if let Some(after) = &parsed.after {
        wheres.push("COALESCE(a.captured_at, a.created_at) > ?".to_string());
        values.push(after.clone());
    }
    if let Some(media_type) = &parsed.media_type {
        wheres.push("a.media_type = ?".to_string());
        values.push(media_type.clone());
    }
    if parsed.favorite_only {
        wheres.push("a.favorite = 1".to_string());
    }

    let sql = format!(
        "SELECT a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width, a.height, a.duration_ms,
                a.created_at, a.captured_at, a.indexed_at, a.favorite, a.rating, a.color_label,
                a.thumbnail_path, a.camera, a.lens, a.deleted_at
         FROM assets a{joins}
         WHERE {where_clause}
         ORDER BY {order_by}
         LIMIT ? OFFSET ?",
        joins = joins,
        where_clause = wheres.join(" AND "),
        order_by = order_by,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut param_refs: Vec<&dyn rusqlite::types::ToSql> = values
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    let limit_s = limit.to_string();
    let offset_s = offset.to_string();
    param_refs.push(&limit_s);
    param_refs.push(&offset_s);

    let rows = stmt.query_map(param_refs.as_slice(), map_asset)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn format_fts_query(text: &str) -> String {
    text.split_whitespace()
        .map(|t| {
            let cleaned: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("{cleaned}*")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn map_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetSummary> {
    Ok(AssetSummary {
        id: row.get(0)?,
        path: row.get(1)?,
        hash: row.get(2)?,
        perceptual_hash: row.get(3)?,
        media_type: row.get(4)?,
        width: row.get(5)?,
        height: row.get(6)?,
        duration_ms: row.get(7)?,
        created_at: row.get(8)?,
        captured_at: row.get(9)?,
        indexed_at: row.get(10)?,
        favorite: row.get::<_, i64>(11)? != 0,
        rating: row.get(12)?,
        color_label: row.get(13)?,
        thumbnail_path: row.get(14)?,
        camera: row.get(15)?,
        lens: row.get(16)?,
        deleted_at: row.get(17)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::indexer;
    use image::{Rgb, RgbImage};
    use rusqlite::params;
    use tempfile::tempdir;

    #[test]
    fn empty_query_browses() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        let rows = search_assets(&conn, "   ", 10, 0).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn lists_only_live_assets_assigned_to_tag() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&media).unwrap();
        RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))
            .save(media.join("tagged.png"))
            .unwrap();
        RgbImage::from_pixel(8, 8, Rgb([4, 5, 6]))
            .save(media.join("untagged.png"))
            .unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        indexer::import_folder_with_progress(&conn, &media, &thumbs, |_, _, _| {}).unwrap();
        let tagged_path = media.join("tagged.png").to_string_lossy().to_string();
        let tagged_id: String = conn
            .query_row(
                "SELECT id FROM assets WHERE path = ?1",
                params![tagged_path],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO tags (id, name) VALUES ('tag-1', 'Receipts')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_tags (asset_id, tag_id) VALUES (?1, 'tag-1')",
            params![tagged_id],
        )
        .unwrap();

        let rows = list_assets_for_tag(&conn, "tag-1", 10, 0).unwrap();

        assert_eq!(rows.len(), 1);
        assert!(rows[0].path.ends_with("tagged.png"));

        conn.execute(
            "UPDATE assets SET deleted_at = '2026-07-26T00:00:00Z' WHERE id = ?1",
            params![tagged_id],
        )
        .unwrap();
        assert!(list_assets_for_tag(&conn, "tag-1", 10, 0)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn browse_filter_ands_across_facets() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&media).unwrap();
        for name in ["a.png", "b.png", "c.png"] {
            RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))
                .save(media.join(name))
                .unwrap();
        }

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        indexer::import_folder_with_progress(&conn, &media, &thumbs, |_, _, _| {}).unwrap();

        let id_for = |name: &str| -> String {
            let path = media.join(name).to_string_lossy().to_string();
            conn.query_row(
                "SELECT id FROM assets WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .unwrap()
        };
        let a = id_for("a.png");
        let b = id_for("b.png");
        let c = id_for("c.png");

        conn.execute("INSERT INTO tags (id, name) VALUES ('t1', 'Alpha')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO asset_tags (asset_id, tag_id) VALUES (?1, 't1'), (?2, 't1')",
            params![a, b],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET rating = 4, color_label = 'yellow' WHERE id = ?1",
            params![a],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET rating = 4, color_label = 'red' WHERE id = ?1",
            params![b],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET rating = 5, color_label = 'yellow' WHERE id = ?1",
            params![c],
        )
        .unwrap();

        let rows = list_assets_for_browse_filter(
            &conn,
            &TagBrowseFilter {
                tag_ids: vec!["t1".into()],
                ratings: vec![4],
                color_labels: vec!["yellow".into()],
            },
            10,
            0,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, a);

        let or_ratings = list_assets_for_browse_filter(
            &conn,
            &TagBrowseFilter {
                tag_ids: vec![],
                ratings: vec![4, 5],
                color_labels: vec!["yellow".into()],
            },
            10,
            0,
        )
        .unwrap();
        assert_eq!(or_ratings.len(), 2);
        let ids: Vec<_> = or_ratings.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&a.as_str()));
        assert!(ids.contains(&c.as_str()));
    }

    #[test]
    fn fts_text_search_joins_on_stored_asset_id() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&media).unwrap();
        RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))
            .save(media.join("sunset.png"))
            .unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        indexer::import_folder_with_progress(&conn, &media, &thumbs, |_, _, _| {}).unwrap();

        let rows = search_assets(&conn, "sunset", 10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].path.ends_with("sunset.png"));
    }
}
