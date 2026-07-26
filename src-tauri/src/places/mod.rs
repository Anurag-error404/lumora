//! On-device Places: GPS EXIF → offline reverse geocode → grouped locations.
//!
//! Every row in `asset_places` is DERIVED: it is rebuilt from the original's
//! GPS EXIF, so dropping the table (and the `places` jobs) leaves a fully
//! working Phase 1 library. Reverse geocoding is fully offline — the
//! `reverse_geocoder` crate bundles GeoNames city data into the binary, so no
//! coordinate ever leaves the machine.

pub mod worker;

use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::models::AssetSummary;
use crate::search::map_asset;

/// `ml_jobs.kind` value used to track which assets have been checked for GPS.
/// A "done" job means the asset was inspected — whether or not it had GPS — so
/// the worker never re-reads the same file.
pub const JOB_KIND: &str = "places";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceGroup {
    /// "City, Region" label from reverse geocoding.
    pub label: String,
    pub country: Option<String>,
    pub asset_count: i64,
    pub cover_thumbnail_path: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacesCoverage {
    pub done: i64,
    pub total: i64,
}

/// Read decimal (lat, lon) from an image's GPS EXIF, if present.
pub fn extract_gps(path: &Path) -> Option<(f64, f64)> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(&file);
    let exif = exif::Reader::new().read_from_container(&mut reader).ok()?;

    let lat = dms_to_decimal(&exif, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef)?;
    let lon = dms_to_decimal(&exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef)?;

    // Reject the common "no fix" sentinel and out-of-range junk.
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    if lat == 0.0 && lon == 0.0 {
        return None;
    }
    Some((lat, lon))
}

fn dms_to_decimal(exif: &exif::Exif, coord: exif::Tag, reference: exif::Tag) -> Option<f64> {
    let field = exif.get_field(coord, exif::In::PRIMARY)?;
    let exif::Value::Rational(ref parts) = field.value else {
        return None;
    };
    if parts.len() < 3 {
        return None;
    }
    let degrees = parts[0].to_f64() + parts[1].to_f64() / 60.0 + parts[2].to_f64() / 3600.0;

    let hemisphere = exif
        .get_field(reference, exif::In::PRIMARY)
        .map(|f| f.display_value().to_string())
        .unwrap_or_default();
    let sign = if hemisphere.starts_with('S') || hemisphere.starts_with('W') {
        -1.0
    } else {
        1.0
    };
    Some(degrees * sign)
}

/// Resolve a coordinate to a "City, Region" label + country code, fully offline.
pub fn reverse_geocode(lat: f64, lon: f64) -> (Option<String>, Option<String>) {
    let result = worker::geocoder().search((lat, lon));
    let record = result.record;
    let city = record.name.trim();
    let region = record.admin1.trim();
    let label = match (city.is_empty(), region.is_empty()) {
        (false, false) => Some(format!("{city}, {region}")),
        (false, true) => Some(city.to_string()),
        (true, false) => Some(region.to_string()),
        (true, true) => None,
    };
    let country = {
        let cc = record.cc.trim();
        if cc.is_empty() {
            None
        } else {
            Some(cc.to_string())
        }
    };
    (label, country)
}

/// Persist a resolved place for an asset and mark its job done.
pub fn store_place(
    conn: &Connection,
    asset_id: &str,
    lat: f64,
    lon: f64,
    label: Option<&str>,
    country: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO asset_places (asset_id, lat, lon, place_label, country, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(asset_id) DO UPDATE SET
           lat = excluded.lat,
           lon = excluded.lon,
           place_label = excluded.place_label,
           country = excluded.country",
        params![
            asset_id,
            lat,
            lon,
            label,
            country,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    mark_job(conn, asset_id, "done", None)
}

/// Record processing state for an asset in the shared `ml_jobs` table.
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
            JOB_KIND,
            state,
            error,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Images that have not yet been checked for GPS (or failed fewer than 3 times).
pub fn pending_assets(conn: &Connection, limit: u32) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path
         FROM assets a
         LEFT JOIN ml_jobs j ON j.asset_id = a.id AND j.kind = ?1
         WHERE a.deleted_at IS NULL
           AND a.media_type = 'image'
           AND (j.state IS NULL OR (j.state = 'failed' AND j.attempts < 3))
         ORDER BY a.indexed_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![JOB_KIND, limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn coverage(conn: &Connection) -> AppResult<PlacesCoverage> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type = 'image'",
        [],
        |r| r.get(0),
    )?;
    let done: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ml_jobs j
         JOIN assets a ON a.id = j.asset_id AND a.deleted_at IS NULL
         WHERE j.kind = ?1 AND j.state = 'done'",
        params![JOB_KIND],
        |r| r.get(0),
    )?;
    Ok(PlacesCoverage { done, total })
}

/// Distinct resolved places, most-populated first, each with a cover thumbnail.
pub fn list_places(conn: &Connection) -> AppResult<Vec<PlaceGroup>> {
    let mut stmt = conn.prepare(
        "SELECT ap.place_label,
                MIN(ap.country),
                COUNT(*) AS cnt,
                AVG(ap.lat),
                AVG(ap.lon),
                (SELECT a2.thumbnail_path
                   FROM asset_places ap2
                   JOIN assets a2 ON a2.id = ap2.asset_id
                  WHERE ap2.place_label = ap.place_label
                    AND a2.deleted_at IS NULL
                    AND a2.thumbnail_path IS NOT NULL
                  ORDER BY COALESCE(a2.captured_at, a2.created_at) DESC
                  LIMIT 1)
         FROM asset_places ap
         JOIN assets a ON a.id = ap.asset_id AND a.deleted_at IS NULL
         WHERE ap.place_label IS NOT NULL AND TRIM(ap.place_label) != ''
         GROUP BY ap.place_label
         ORDER BY cnt DESC, ap.place_label ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PlaceGroup {
            label: r.get(0)?,
            country: r.get(1)?,
            asset_count: r.get(2)?,
            lat: r.get(3)?,
            lon: r.get(4)?,
            cover_thumbnail_path: r.get(5)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn list_place_assets(
    conn: &Connection,
    label: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width, a.height,
                a.duration_ms, a.created_at, a.captured_at, a.indexed_at, a.favorite,
                a.rating, a.color_label, a.thumbnail_path, a.camera, a.lens, a.deleted_at
         FROM assets a
         JOIN asset_places ap ON ap.asset_id = a.id
         WHERE a.deleted_at IS NULL AND ap.place_label = ?1
         ORDER BY COALESCE(a.captured_at, a.created_at) DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(params![label, limit, offset], map_asset)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Drop all Places data so it can be rebuilt from originals. Reversible: only
/// derived rows are removed; originals and library metadata are untouched.
pub fn clear_all(conn: &Connection) -> AppResult<usize> {
    let n = conn.execute("DELETE FROM asset_places", [])?;
    conn.execute("DELETE FROM ml_jobs WHERE kind = ?1", params![JOB_KIND])?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    fn seed_asset(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, thumbnail_path)
             VALUES (?1, ?2, ?3, 'image', datetime('now'), datetime('now'), ?4)",
            params![id, format!("/tmp/{id}.jpg"), id, format!("/thumbs/{id}.jpg")],
        )
        .unwrap();
    }

    #[test]
    fn store_and_group_places() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        seed_asset(&conn, "a2");
        seed_asset(&conn, "a3");

        store_place(&conn, "a1", 40.0, -73.0, Some("New York, New York"), Some("US")).unwrap();
        store_place(&conn, "a2", 40.1, -73.1, Some("New York, New York"), Some("US")).unwrap();
        store_place(&conn, "a3", 48.85, 2.35, Some("Paris, Île-de-France"), Some("FR")).unwrap();

        let groups = list_places(&conn).unwrap();
        assert_eq!(groups.len(), 2);
        // Most-populated place first.
        assert_eq!(groups[0].label, "New York, New York");
        assert_eq!(groups[0].asset_count, 2);
        assert_eq!(groups[0].country.as_deref(), Some("US"));

        let ny = list_place_assets(&conn, "New York, New York", 50, 0).unwrap();
        assert_eq!(ny.len(), 2);
    }

    #[test]
    fn assets_without_a_label_are_not_grouped() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        // GPS present but nothing resolved (e.g. mid-ocean).
        store_place(&conn, "a1", 0.5, -30.0, None, None).unwrap();

        assert!(list_places(&conn).unwrap().is_empty());
        // The job is still "done" so it will not be reprocessed forever.
        let cov = coverage(&conn).unwrap();
        assert_eq!(cov.done, 1);
    }

    #[test]
    fn pending_skips_done_and_clear_resets() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        assert_eq!(pending_assets(&conn, 10).unwrap().len(), 1);

        store_place(&conn, "a1", 40.0, -73.0, Some("New York, New York"), Some("US")).unwrap();
        assert!(pending_assets(&conn, 10).unwrap().is_empty());

        let removed = clear_all(&conn).unwrap();
        assert_eq!(removed, 1);
        assert!(list_places(&conn).unwrap().is_empty());
        // Cleared jobs mean the asset is pending again for a rebuild.
        assert_eq!(pending_assets(&conn, 10).unwrap().len(), 1);
    }

    #[test]
    fn cascade_delete_removes_place() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        store_place(&conn, "a1", 40.0, -73.0, Some("New York, New York"), Some("US")).unwrap();
        conn.execute("DELETE FROM assets WHERE id = 'a1'", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM asset_places", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn reverse_geocode_resolves_a_known_city() {
        // Manhattan should resolve to a US place with a non-empty label.
        let (label, country) = reverse_geocode(40.7831, -73.9712);
        assert!(label.is_some(), "expected a label for NYC coordinates");
        assert_eq!(country.as_deref(), Some("US"));
    }
}
