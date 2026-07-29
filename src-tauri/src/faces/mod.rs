//! On-device faces: SCRFD detection + ArcFace embeddings + clustering.

pub mod cluster;
pub mod engine;
pub mod worker;

use std::path::{Path, PathBuf};

use image::ImageFormat;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::faces::engine::DetectedFace;
use crate::indexer;
use crate::ml::{self, catalog::ModelKind, vector};
use crate::models::AssetSummary;
use crate::search::map_asset;

#[derive(Debug, Clone)]
pub struct FaceModelPaths {
    pub det: PathBuf,
    pub rec: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacesCoverage {
    pub done: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: String,
    pub name: Option<String>,
    pub face_count: i64,
    pub cover_crop_path: Option<String>,
    pub created_at: String,
    pub ignored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceBox {
    pub id: String,
    pub asset_id: String,
    pub person_id: Option<String>,
    pub person_name: Option<String>,
    pub bbox_x: f32,
    pub bbox_y: f32,
    pub bbox_w: f32,
    pub bbox_h: f32,
    pub score: f32,
    pub crop_path: Option<String>,
    pub person_ignored: bool,
}

/// Active faces bundle from preferences (falls back to buffalo_l).
pub fn active_bundle(app_data: &Path) -> String {
    let preferred = crate::preferences::load(app_data)
        .map(|p| p.ai.faces_model)
        .unwrap_or_else(|_| "insightface-buffalo-l".into());
    let opt = ml::library::resolve_active(ml::library::Capability::Faces, &preferred);
    opt.bundle.unwrap_or(ml::catalog::FACES_BUNDLE).to_string()
}

pub fn faces_ready(conn: &Connection) -> AppResult<bool> {
    faces_ready_bundle(conn, ml::catalog::FACES_BUNDLE)
}

pub fn faces_ready_bundle(conn: &Connection, bundle: &str) -> AppResult<bool> {
    for entry in ml::catalog::bundle(bundle) {
        if ml::installed_row(conn, entry.id)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn model_paths_for(conn: &Connection, bundle: &str) -> AppResult<FaceModelPaths> {
    let mut det = None;
    let mut rec = None;
    for entry in ml::catalog::bundle(bundle) {
        match entry.kind {
            ModelKind::FaceDetect => det = Some(ml::require_path(conn, entry.id)?),
            ModelKind::FaceEmbed => rec = Some(ml::require_path(conn, entry.id)?),
            _ => {}
        }
    }
    Ok(FaceModelPaths {
        det: det.ok_or_else(|| {
            crate::error::AppError::msg(format!("face detect model missing in bundle {bundle}"))
        })?,
        rec: rec.ok_or_else(|| {
            crate::error::AppError::msg(format!("face embed model missing in bundle {bundle}"))
        })?,
    })
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
            ModelKind::Faces.as_str(),
            state,
            error,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Persist detections for an asset: write crops, assign people, store rows.
pub fn store_detections(
    conn: &Connection,
    faces_dir: &Path,
    asset_id: &str,
    detections: &[DetectedFace],
) -> AppResult<()> {
    std::fs::create_dir_all(faces_dir)?;
    // Replace any previous faces for this asset.
    invalidate_asset_rows(conn, faces_dir, asset_id)?;

    let now = chrono::Utc::now().to_rfc3339();
    for det in detections {
        let face_id = uuid::Uuid::new_v4().to_string();
        let crop_file = faces_dir.join(format!("{face_id}.jpg"));
        let rgb = image::DynamicImage::ImageRgba8(det.crop.clone()).to_rgb8();
        let crop_path = match rgb.save_with_format(&crop_file, ImageFormat::Jpeg) {
            Ok(()) => Some(crop_file.display().to_string()),
            Err(e) => {
                tracing::warn!(
                    asset = %asset_id,
                    face = %face_id,
                    error = %e,
                    "face crop save skipped"
                );
                None
            }
        };

        let assigned = cluster::assign(conn, &det.embedding)?;
        if assigned.created {
            tracing::debug!(person = %assigned.person_id, "created new face cluster");
        }
        conn.execute(
            "INSERT INTO faces (id, asset_id, person_id, bbox_x, bbox_y, bbox_w, bbox_h,
                                score, embedding, crop_path, detected_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                face_id,
                asset_id,
                assigned.person_id,
                det.x as f64,
                det.y as f64,
                det.w as f64,
                det.h as f64,
                det.score as f64,
                vector::encode(&det.embedding),
                crop_path,
                now
            ],
        )?;
        cluster::refresh_person_stats(conn, &assigned.person_id)?;
    }

    // Zero faces is still a successful pass.
    mark_job(conn, asset_id, "done", None)?;
    indexer::refresh_fts(conn, asset_id)?;
    Ok(())
}

pub fn pending_assets(conn: &Connection, limit: u32) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path
         FROM assets a
         LEFT JOIN ml_jobs j
           ON j.asset_id = a.id AND j.kind = ?1
         WHERE a.deleted_at IS NULL
           AND a.media_type = 'image'
           AND (j.state IS NULL OR (j.state = 'failed' AND j.attempts < 3))
         ORDER BY a.indexed_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![ModelKind::Faces.as_str(), limit], |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn coverage(conn: &Connection) -> AppResult<FacesCoverage> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type = 'image'",
        [],
        |r| r.get(0),
    )?;
    let done: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ml_jobs j
         JOIN assets a ON a.id = j.asset_id AND a.deleted_at IS NULL
         WHERE j.kind = ?1 AND j.state = 'done'",
        params![ModelKind::Faces.as_str()],
        |r| r.get(0),
    )?;
    Ok(FacesCoverage { done, total })
}

pub fn list_people(conn: &Connection) -> AppResult<Vec<Person>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.face_count, p.created_at, p.ignored,
                (SELECT f.crop_path FROM faces f WHERE f.id = p.cover_face_id)
         FROM people p
         WHERE p.face_count > 0 AND p.ignored = 0
         ORDER BY
           CASE WHEN p.name IS NULL OR TRIM(p.name) = '' THEN 1 ELSE 0 END,
           LOWER(COALESCE(p.name, '')),
           p.face_count DESC",
    )?;
    let rows = stmt.query_map([], map_person)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// People the user chose to ignore, so they can be restored later.
pub fn list_ignored_people(conn: &Connection) -> AppResult<Vec<Person>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, p.face_count, p.created_at, p.ignored,
                (SELECT f.crop_path FROM faces f WHERE f.id = p.cover_face_id)
         FROM people p
         WHERE p.ignored = 1
         ORDER BY p.face_count DESC, p.created_at DESC",
    )?;
    let rows = stmt.query_map([], map_person)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn map_person(r: &rusqlite::Row<'_>) -> rusqlite::Result<Person> {
    Ok(Person {
        id: r.get(0)?,
        name: r.get(1)?,
        face_count: r.get(2)?,
        created_at: r.get(3)?,
        ignored: r.get::<_, i64>(4)? != 0,
        cover_crop_path: existing_crop_path(r.get(5)?),
    })
}

/// Return the path only when the JPEG still exists on disk.
fn existing_crop_path(path: Option<String>) -> Option<String> {
    path.filter(|p| Path::new(p).is_file())
}

/// Clear stale `crop_path` rows and refresh person covers after cache wipes.
pub fn repair_missing_face_crops(conn: &Connection) -> AppResult<u32> {
    let rows: Vec<(String, Option<String>, Option<String>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, crop_path, person_id FROM faces
             WHERE crop_path IS NOT NULL AND crop_path != ''",
        )?;
        let mapped = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        mapped.filter_map(|r| r.ok()).collect()
    };

    let mut cleared = 0u32;
    let mut people = std::collections::HashSet::new();
    for (face_id, crop_path, person_id) in rows {
        let missing = match &crop_path {
            Some(p) => !Path::new(p).is_file(),
            None => false,
        };
        if !missing {
            continue;
        }
        conn.execute(
            "UPDATE faces SET crop_path = NULL WHERE id = ?1",
            params![face_id],
        )?;
        cleared += 1;
        if let Some(pid) = person_id {
            people.insert(pid);
        }
    }
    for person_id in people {
        let _ = cluster::refresh_person_stats(conn, &person_id);
    }
    Ok(cleared)
}

/// Hide (or restore) a person. The cluster and its centroid are kept so future
/// detections of the same face keep landing here instead of resurfacing.
pub fn set_ignored(conn: &Connection, person_id: &str, ignored: bool) -> AppResult<()> {
    let changed = conn.execute(
        "UPDATE people SET ignored = ?2, updated_at = ?3 WHERE id = ?1",
        params![
            person_id,
            if ignored { 1 } else { 0 },
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    if changed == 0 {
        return Ok(());
    }
    let asset_ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT asset_id FROM faces WHERE person_id = ?1")?;
        let rows = stmt.query_map(params![person_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    for id in &asset_ids {
        indexer::refresh_fts(conn, id)?;
    }
    Ok(())
}

pub fn list_person_assets(
    conn: &Connection,
    person_id: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width, a.height,
                a.duration_ms, a.created_at, a.captured_at, a.indexed_at, a.favorite,
                a.rating, a.color_label, a.thumbnail_path, a.camera, a.lens, a.deleted_at
         FROM assets a
         WHERE a.deleted_at IS NULL
           AND EXISTS (
             SELECT 1 FROM faces f WHERE f.asset_id = a.id AND f.person_id = ?1
           )
         ORDER BY COALESCE(a.captured_at, a.created_at) DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(params![person_id, limit, offset], map_asset)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn list_asset_faces(conn: &Connection, asset_id: &str) -> AppResult<Vec<FaceBox>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.asset_id, f.person_id, p.name,
                f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h, f.score, f.crop_path,
                COALESCE(p.ignored, 0)
         FROM faces f
         LEFT JOIN people p ON p.id = f.person_id
         WHERE f.asset_id = ?1
         ORDER BY f.score DESC",
    )?;
    let rows = stmt.query_map(params![asset_id], |r| {
        Ok(FaceBox {
            id: r.get(0)?,
            asset_id: r.get(1)?,
            person_id: r.get(2)?,
            person_name: r.get(3)?,
            bbox_x: r.get::<_, f64>(4)? as f32,
            bbox_y: r.get::<_, f64>(5)? as f32,
            bbox_w: r.get::<_, f64>(6)? as f32,
            bbox_h: r.get::<_, f64>(7)? as f32,
            score: r.get::<_, f64>(8)? as f32,
            crop_path: existing_crop_path(r.get(9)?),
            person_ignored: r.get::<_, i64>(10)? != 0,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn clear_all(conn: &Connection, faces_dir: &Path) -> AppResult<usize> {
    let asset_ids: Vec<String> = {
        let mut stmt = conn.prepare("SELECT DISTINCT asset_id FROM faces")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let n = conn.execute("DELETE FROM faces", [])?;
    conn.execute("DELETE FROM people", [])?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE kind = ?1",
        params![ModelKind::Faces.as_str()],
    )?;
    if faces_dir.exists() {
        let _ = std::fs::remove_dir_all(faces_dir);
        let _ = std::fs::create_dir_all(faces_dir);
    }
    for id in &asset_ids {
        let _ = indexer::refresh_fts(conn, id);
    }
    Ok(n)
}

fn invalidate_asset_rows(conn: &Connection, faces_dir: &Path, asset_id: &str) -> AppResult<()> {
    let crops: Vec<Option<String>> = {
        let mut stmt = conn.prepare("SELECT crop_path FROM faces WHERE asset_id = ?1")?;
        let rows = stmt.query_map(params![asset_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let person_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT person_id FROM faces WHERE asset_id = ?1 AND person_id IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![asset_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    conn.execute("DELETE FROM faces WHERE asset_id = ?1", params![asset_id])?;
    for path in crops.into_iter().flatten() {
        let p = PathBuf::from(&path);
        if p.starts_with(faces_dir) && p.exists() {
            let _ = std::fs::remove_file(p);
        }
    }
    for pid in person_ids {
        let remaining: i64 = conn.query_row(
            "SELECT COUNT(*) FROM faces WHERE person_id = ?1",
            params![pid],
            |r| r.get(0),
        )?;
        if remaining == 0 {
            // Ignored clusters outlive their faces: dropping the row would make
            // the same face reappear the next time it is detected.
            conn.execute(
                "DELETE FROM people WHERE id = ?1 AND ignored = 0",
                params![pid],
            )?;
            cluster::refresh_person_stats(conn, &pid)?;
        } else {
            cluster::rebuild_centroid(conn, &pid)?;
            cluster::refresh_person_stats(conn, &pid)?;
        }
    }
    Ok(())
}

/// Drop faces for one asset and clear its faces job so it will be reprocessed.
pub fn invalidate_asset(conn: &Connection, faces_dir: &Path, asset_id: &str) -> AppResult<()> {
    invalidate_asset_rows(conn, faces_dir, asset_id)?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE asset_id = ?1 AND kind = ?2",
        params![asset_id, ModelKind::Faces.as_str()],
    )?;
    indexer::refresh_fts(conn, asset_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    fn seed_asset(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES (?1, ?2, ?3, 'image', datetime('now'), datetime('now'))",
            params![id, format!("/tmp/{id}.jpg"), id],
        )
        .unwrap();
    }

    #[test]
    fn pending_honours_three_attempt_cap() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        assert_eq!(pending_assets(&conn, 50).unwrap().len(), 1);
        for _ in 0..3 {
            mark_job(&conn, "a1", "failed", Some("boom")).unwrap();
        }
        assert!(pending_assets(&conn, 50).unwrap().is_empty());
    }

    #[test]
    fn store_and_name_makes_fts_find_person() {
        let dir = tempdir().unwrap();
        let faces_dir = dir.path().join("faces");
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");

        let mut emb = vec![0.0f32; 512];
        emb[0] = 1.0;
        let crop = image::RgbaImage::from_pixel(16, 16, image::Rgba([10, 20, 30, 255]));
        let det = DetectedFace {
            x: 1.0,
            y: 2.0,
            w: 10.0,
            h: 10.0,
            score: 0.9,
            kps: [[0.0; 2]; 5],
            embedding: emb,
            crop,
        };
        store_detections(&conn, &faces_dir, "a1", &[det]).unwrap();
        let people = list_people(&conn).unwrap();
        assert_eq!(people.len(), 1);
        cluster::rename(&conn, &people[0].id, "Jordan").unwrap();
        indexer::refresh_fts(&conn, "a1").unwrap();

        let found = crate::search::search_assets(&conn, "Jordan", 10, 0).unwrap();
        assert!(
            found.iter().any(|a| a.id == "a1"),
            "named person should be FTS-searchable"
        );
    }

    fn face(seed: usize, score: f32) -> DetectedFace {
        let mut emb = vec![0.0f32; 512];
        emb[seed] = 1.0;
        DetectedFace {
            x: 0.0,
            y: 0.0,
            w: 8.0,
            h: 8.0,
            score,
            kps: [[0.0; 2]; 5],
            embedding: emb,
            crop: image::RgbaImage::from_pixel(8, 8, image::Rgba([9, 9, 9, 255])),
        }
    }

    #[test]
    fn ignored_person_stays_hidden_when_the_face_returns() {
        let dir = tempdir().unwrap();
        let faces_dir = dir.path().join("faces");
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        seed_asset(&conn, "a2");

        store_detections(&conn, &faces_dir, "a1", &[face(7, 0.9)]).unwrap();
        let person = list_people(&conn).unwrap().remove(0);
        set_ignored(&conn, &person.id, true).unwrap();

        assert!(list_people(&conn).unwrap().is_empty());
        let ignored = list_ignored_people(&conn).unwrap();
        assert_eq!(ignored.len(), 1);
        assert!(ignored[0].ignored);

        // A later import of the same face must land back in the ignored cluster.
        store_detections(&conn, &faces_dir, "a2", &[face(7, 0.8)]).unwrap();
        assert!(
            list_people(&conn).unwrap().is_empty(),
            "an ignored face should not resurface as a new person"
        );
        assert_eq!(list_ignored_people(&conn).unwrap()[0].face_count, 2);

        // Reclustering and restoring both leave the cluster intact.
        cluster::recluster_unnamed(&conn).unwrap();
        assert!(list_people(&conn).unwrap().is_empty());
        set_ignored(&conn, &person.id, false).unwrap();
        assert_eq!(list_people(&conn).unwrap().len(), 1);
    }

    #[test]
    fn ignored_names_are_excluded_from_search() {
        let dir = tempdir().unwrap();
        let faces_dir = dir.path().join("faces");
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        store_detections(&conn, &faces_dir, "a1", &[face(11, 0.9)]).unwrap();
        let person = list_people(&conn).unwrap().remove(0);
        cluster::rename(&conn, &person.id, "Priya").unwrap();
        indexer::refresh_fts(&conn, "a1").unwrap();
        assert!(!crate::search::search_assets(&conn, "Priya", 10, 0)
            .unwrap()
            .is_empty());

        set_ignored(&conn, &person.id, true).unwrap();
        assert!(
            crate::search::search_assets(&conn, "Priya", 10, 0)
                .unwrap()
                .is_empty(),
            "ignored people should drop out of FTS"
        );
    }

    #[test]
    fn ignored_cluster_survives_losing_all_its_faces() {
        let dir = tempdir().unwrap();
        let faces_dir = dir.path().join("faces");
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        store_detections(&conn, &faces_dir, "a1", &[face(5, 0.9)]).unwrap();
        let person = list_people(&conn).unwrap().remove(0);
        set_ignored(&conn, &person.id, true).unwrap();

        invalidate_asset(&conn, &faces_dir, "a1").unwrap();
        let ignored = list_ignored_people(&conn).unwrap();
        assert_eq!(
            ignored.len(),
            1,
            "ignore must outlive the faces that created it"
        );
        let centroid: Option<Vec<u8>> = conn
            .query_row(
                "SELECT centroid FROM people WHERE id = ?1",
                params![person.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            centroid.is_some(),
            "centroid is what keeps the face ignored"
        );

        store_detections(&conn, &faces_dir, "a1", &[face(5, 0.9)]).unwrap();
        assert!(list_people(&conn).unwrap().is_empty());
    }

    #[test]
    fn cascade_delete_removes_faces() {
        let dir = tempdir().unwrap();
        let faces_dir = dir.path().join("faces");
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "a1");
        let mut emb = vec![0.0f32; 512];
        emb[3] = 1.0;
        let crop = image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]));
        store_detections(
            &conn,
            &faces_dir,
            "a1",
            &[DetectedFace {
                x: 0.0,
                y: 0.0,
                w: 5.0,
                h: 5.0,
                score: 0.8,
                kps: [[0.0; 2]; 5],
                embedding: emb,
                crop,
            }],
        )
        .unwrap();
        conn.execute("DELETE FROM assets WHERE id = 'a1'", [])
            .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM faces", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
