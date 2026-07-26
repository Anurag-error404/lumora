//! Smart collections: standing library filters surfaced as sidebar
//! destinations. Each one is a query over `assets` — there is no stored
//! membership, so collections stay correct as the library changes.
//!
//! A collection contributes a predicate and, optionally, JOIN clauses. The
//! JOIN hook is what lets Phase 2 collections sit on derived tables (OCR text,
//! embeddings) without every collection paying for the join.

use rusqlite::{Connection, ToSql};

use crate::error::{AppError, AppResult};
use crate::indexer::scan::RAW_EXT;
use crate::models::AssetSummary;
use crate::search::map_asset;

const SELECT_COLUMNS: &str = "a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width,
     a.height, a.duration_ms, a.created_at, a.captured_at, a.indexed_at, a.favorite,
     a.rating, a.color_label, a.thumbnail_path, a.camera, a.lens, a.deleted_at";

/// Long edge / short edge at or above this is treated as a panorama. Wide enough
/// to exclude ordinary 16:9 captures (1.78) and 2:1 crops sit right at the line.
const PANORAMA_ASPECT: f64 = 2.0;

/// The SQL fragments a collection contributes, minus the trash filter and paging.
///
/// `joins` is the extension point for Phase 2: OCR- and embedding-backed
/// collections join their derived table here, while metadata-only collections
/// pay nothing for it.
struct CollectionQuery {
    joins: &'static str,
    predicate: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartCollection {
    Videos,
    RawPhotos,
    Screenshots,
    Selfies,
    Panoramas,
}

impl SmartCollection {
    pub const ALL: [Self; 5] = [
        Self::Videos,
        Self::RawPhotos,
        Self::Screenshots,
        Self::Selfies,
        Self::Panoramas,
    ];

    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "videos" => Ok(Self::Videos),
            "rawPhotos" | "raw" => Ok(Self::RawPhotos),
            "screenshots" => Ok(Self::Screenshots),
            "selfies" => Ok(Self::Selfies),
            "panoramas" => Ok(Self::Panoramas),
            other => Err(AppError::msg(format!("unknown smart collection: {other}"))),
        }
    }

    /// Stable identifier shared with the frontend `SmartCollectionKind` union.
    pub fn id(self) -> &'static str {
        match self {
            Self::Videos => "videos",
            Self::RawPhotos => "rawPhotos",
            Self::Screenshots => "screenshots",
            Self::Selfies => "selfies",
            Self::Panoramas => "panoramas",
        }
    }

    fn query(self) -> CollectionQuery {
        let predicate = match self {
            Self::Videos => "a.media_type = 'video'".to_string(),
            // Matched on extension: RAW files usually can't be decoded, so
            // there is no metadata signal to rely on.
            Self::RawPhotos => {
                let exts = RAW_EXT
                    .iter()
                    .map(|ext| format!("LOWER(a.path) LIKE '%.{ext}'"))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                format!("a.media_type = 'image' AND ({exts})")
            }
            // Screenshots carry no camera EXIF, and both macOS and Windows name
            // them (or their folder) predictably. Requiring both keeps real
            // photos that merely live in a "Screenshots" folder out.
            Self::Screenshots => "a.media_type = 'image'
                 AND a.camera IS NULL
                 AND (LOWER(a.path) LIKE '%screenshot%'
                      OR LOWER(a.path) LIKE '%screen shot%')"
                .to_string(),
            // Phones record the front camera in the lens description
            // ("iPhone 15 Pro front camera 2.69mm f/2.2"). The filename fallback
            // catches exports that dropped the lens tag.
            Self::Selfies => "a.media_type = 'image'
                 AND (LOWER(COALESCE(a.lens, '')) LIKE '%front%'
                      OR LOWER(a.path) LIKE '%selfie%')"
                .to_string(),
            // Orientation-agnostic: stitched verticals are panoramas too.
            Self::Panoramas => format!(
                "a.media_type = 'image'
                 AND a.width IS NOT NULL AND a.height IS NOT NULL
                 AND MIN(a.width, a.height) > 0
                 AND (CAST(MAX(a.width, a.height) AS REAL)
                      / MIN(a.width, a.height)) >= {PANORAMA_ASPECT}"
            ),
        };
        CollectionQuery {
            joins: "",
            predicate,
        }
    }
}

pub fn list(
    conn: &Connection,
    kind: SmartCollection,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let CollectionQuery { joins, predicate } = kind.query();
    let sql = format!(
        "SELECT {SELECT_COLUMNS}
         FROM assets a
         {joins}
         WHERE a.deleted_at IS NULL AND ({predicate})
         ORDER BY COALESCE(a.captured_at, a.created_at) DESC
         LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params: [&dyn ToSql; 2] = [&limit, &offset];
    let rows = stmt.query_map(params.as_slice(), map_asset)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn count(conn: &Connection, kind: SmartCollection) -> AppResult<i64> {
    let CollectionQuery { joins, predicate } = kind.query();
    let sql = format!(
        "SELECT COUNT(*) FROM assets a {joins}
         WHERE a.deleted_at IS NULL AND ({predicate})"
    );
    Ok(conn.query_row(&sql, [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::params;
    use tempfile::tempdir;

    fn seed(conn: &Connection, id: &str, path: &str, media_type: &str, camera: Option<&str>) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, camera)
             VALUES (?1, ?2, ?3, ?4, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', ?5)",
            params![id, path, format!("h-{id}"), media_type, camera],
        )
        .unwrap();
    }

    fn seed_lens(conn: &Connection, id: &str, path: &str, lens: Option<&str>) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, lens)
             VALUES (?1, ?2, ?3, 'image', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', ?4)",
            params![id, path, format!("h-{id}"), lens],
        )
        .unwrap();
    }

    fn seed_dims(conn: &Connection, id: &str, media_type: &str, width: i64, height: i64) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, width, height)
             VALUES (?1, ?2, ?3, ?4, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', ?5, ?6)",
            params![
                id,
                format!("/m/{id}.jpg"),
                format!("h-{id}"),
                media_type,
                width,
                height
            ],
        )
        .unwrap();
    }

    fn ids(rows: &[AssetSummary]) -> Vec<&str> {
        rows.iter().map(|a| a.id.as_str()).collect()
    }

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn videos_collection_matches_only_video_assets() {
        let (_dir, conn) = open();
        seed(&conn, "v1", "/m/clip.mp4", "video", None);
        seed(&conn, "v2", "/m/clip.mov", "video", None);
        seed(&conn, "i1", "/m/photo.jpg", "image", Some("Canon"));

        let rows = list(&conn, SmartCollection::Videos, 50, 0).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|a| a.media_type == "video"));
        assert_eq!(count(&conn, SmartCollection::Videos).unwrap(), 2);
    }

    #[test]
    fn raw_collection_matches_by_extension_and_ignores_jpeg() {
        let (_dir, conn) = open();
        seed(&conn, "r1", "/m/shot.CR2", "image", Some("Canon"));
        seed(&conn, "r2", "/m/shot.dng", "image", None);
        seed(&conn, "r3", "/m/shot.nef", "image", None);
        seed(&conn, "j1", "/m/shot.jpg", "image", Some("Canon"));
        // A folder named "raw" must not pull in non-RAW files.
        seed(&conn, "j2", "/m/raw/holiday.jpg", "image", None);

        let rows = list(&conn, SmartCollection::RawPhotos, 50, 0).unwrap();
        let ids: Vec<&str> = rows.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(rows.len(), 3, "got {ids:?}");
        assert!(
            ids.contains(&"r1"),
            "extension match must be case-insensitive"
        );
        assert!(!ids.contains(&"j1") && !ids.contains(&"j2"));
    }

    #[test]
    fn screenshots_collection_excludes_camera_photos() {
        let (_dir, conn) = open();
        seed(
            &conn,
            "s1",
            "/m/Screenshot 2026-01-01 at 10.00.png",
            "image",
            None,
        );
        seed(&conn, "s2", "/m/Screenshots/capture.png", "image", None);
        // Same folder, but a real photo: camera EXIF rules it out.
        seed(
            &conn,
            "p1",
            "/m/Screenshots/beach.jpg",
            "image",
            Some("iPhone"),
        );
        seed(&conn, "p2", "/m/holiday.jpg", "image", None);

        let rows = list(&conn, SmartCollection::Screenshots, 50, 0).unwrap();
        let ids: Vec<&str> = rows.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(rows.len(), 2, "got {ids:?}");
        assert!(ids.contains(&"s1") && ids.contains(&"s2"));
        assert!(!ids.contains(&"p1") && !ids.contains(&"p2"));
    }

    #[test]
    fn selfies_match_front_camera_lens_or_filename() {
        let (_dir, conn) = open();
        seed_lens(
            &conn,
            "s1",
            "/m/IMG_001.jpg",
            Some("iPhone 15 Pro front camera 2.69mm f/2.2"),
        );
        seed_lens(&conn, "s2", "/m/selfie-with-dog.jpg", None);
        seed_lens(
            &conn,
            "b1",
            "/m/IMG_002.jpg",
            Some("iPhone 15 Pro back camera 6.86mm f/1.78"),
        );
        seed_lens(&conn, "b2", "/m/landscape.jpg", None);

        let rows = list(&conn, SmartCollection::Selfies, 50, 0).unwrap();
        let got = ids(&rows);
        assert_eq!(rows.len(), 2, "got {got:?}");
        assert!(got.contains(&"s1") && got.contains(&"s2"));
        assert!(!got.contains(&"b1"), "back camera must not count as a selfie");
        assert_eq!(count(&conn, SmartCollection::Selfies).unwrap(), 2);
    }

    #[test]
    fn panoramas_match_either_orientation_and_skip_ordinary_frames() {
        let (_dir, conn) = open();
        seed_dims(&conn, "wide", "image", 12000, 3000);
        seed_dims(&conn, "tall", "image", 2000, 6000);
        // Exactly at the 2.0 threshold — inclusive.
        seed_dims(&conn, "edge", "image", 4000, 2000);
        // Ordinary 16:9 and 4:3 frames stay out.
        seed_dims(&conn, "wide169", "image", 1920, 1080);
        seed_dims(&conn, "normal", "image", 4000, 3000);
        // A very wide video is not a panorama.
        seed_dims(&conn, "clip", "video", 8000, 2000);

        let rows = list(&conn, SmartCollection::Panoramas, 50, 0).unwrap();
        let got = ids(&rows);
        assert_eq!(rows.len(), 3, "got {got:?}");
        assert!(got.contains(&"wide") && got.contains(&"tall") && got.contains(&"edge"));
        assert!(!got.contains(&"wide169") && !got.contains(&"normal"));
        assert!(!got.contains(&"clip"), "videos are never panoramas");
    }

    #[test]
    fn panoramas_ignore_assets_without_dimensions() {
        let (_dir, conn) = open();
        // RAW files often index without decodable dimensions.
        seed(&conn, "r1", "/m/shot.cr2", "image", None);

        assert!(list(&conn, SmartCollection::Panoramas, 50, 0)
            .unwrap()
            .is_empty());
        assert_eq!(count(&conn, SmartCollection::Panoramas).unwrap(), 0);
    }

    #[test]
    fn every_collection_round_trips_through_its_id() {
        for kind in SmartCollection::ALL {
            assert_eq!(
                SmartCollection::parse(kind.id()).unwrap(),
                kind,
                "id() and parse() disagree for {kind:?}"
            );
        }
    }

    #[test]
    fn every_collection_produces_runnable_sql_on_an_empty_library() {
        let (_dir, conn) = open();
        for kind in SmartCollection::ALL {
            list(&conn, kind, 10, 0)
                .unwrap_or_else(|e| panic!("list failed for {kind:?}: {e}"));
            let n = count(&conn, kind)
                .unwrap_or_else(|e| panic!("count failed for {kind:?}: {e}"));
            assert_eq!(n, 0);
        }
    }

    #[test]
    fn collections_hide_trashed_assets_and_reject_unknown_names() {
        let (_dir, conn) = open();
        seed(&conn, "v1", "/m/clip.mp4", "video", None);
        conn.execute(
            "UPDATE assets SET deleted_at = '2026-07-01T00:00:00Z' WHERE id = 'v1'",
            [],
        )
        .unwrap();

        assert!(list(&conn, SmartCollection::Videos, 50, 0)
            .unwrap()
            .is_empty());
        assert!(SmartCollection::parse("people").is_err());
        assert_eq!(
            SmartCollection::parse("rawPhotos").unwrap(),
            SmartCollection::RawPhotos
        );
    }
}
