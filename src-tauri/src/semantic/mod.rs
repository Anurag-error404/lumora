//! Semantic search over CLIP embeddings.
//!
//! This module owns everything *except* running the model: storing vectors,
//! tracking which assets still need one, and ranking a query vector against
//! the library. Keeping inference out means the whole search path is testable
//! with synthetic vectors and no model on disk.
//!
//! Ranking is a brute-force scan. At the sizes we can actually measure today
//! that is fast — 512 floats per asset is a 2 KB dot product — and it avoids
//! committing to an ANN index before there is evidence one is needed.

pub mod worker;

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::ml::{self, catalog::ModelKind, vector};

/// The model whose vectors we search. Stored per row so a future model can be
/// backfilled alongside the current one instead of invalidating the library.
pub const IMAGE_MODEL_ID: &str = "clip-vit-b32-image";
pub const TEXT_MODEL_ID: &str = "clip-vit-b32-text";
pub const TOKENIZER_ID: &str = "clip-vit-b32-tokenizer";

/// Below this cosine score a CLIP match is effectively noise. Without a floor,
/// a query with no real matches still returns the whole library in some order.
pub const MIN_SCORE: f32 = 0.15;

/// Resolved on-disk paths for the semantic bundle.
#[derive(Debug)]
pub struct SemanticModelPaths {
    pub image: std::path::PathBuf,
    pub text: std::path::PathBuf,
    pub tokenizer: std::path::PathBuf,
}

/// Locate every file inference needs, or explain which one is missing.
pub fn model_paths(conn: &Connection) -> AppResult<SemanticModelPaths> {
    Ok(SemanticModelPaths {
        image: ml::require_path(conn, IMAGE_MODEL_ID)?,
        text: ml::require_path(conn, TEXT_MODEL_ID)?,
        tokenizer: ml::require_path(conn, TOKENIZER_ID)?,
    })
}

/// Store an embedding for an asset, normalising it first so later comparisons
/// are a plain dot product.
pub fn store(conn: &Connection, asset_id: &str, model_id: &str, embedding: &[f32]) -> AppResult<()> {
    let mut v = embedding.to_vec();
    vector::normalize(&mut v);
    conn.execute(
        "INSERT INTO asset_embeddings (asset_id, model_id, dim, vector, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(asset_id, model_id) DO UPDATE SET
           dim = excluded.dim,
           vector = excluded.vector,
           created_at = excluded.created_at",
        params![
            asset_id,
            model_id,
            v.len() as i64,
            vector::encode(&v),
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    mark_job(conn, asset_id, ModelKind::ClipImage, "done", None)?;
    Ok(())
}

/// Record per-asset processing state so an interrupted run can resume.
pub fn mark_job(
    conn: &Connection,
    asset_id: &str,
    kind: ModelKind,
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
            kind.as_str(),
            state,
            error,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Images that still need an embedding.
///
/// Repeatedly failing assets are excluded so one undecodable file cannot stall
/// the queue forever.
pub fn pending_assets(conn: &Connection, limit: u32) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.path
         FROM assets a
         LEFT JOIN asset_embeddings e
           ON e.asset_id = a.id AND e.model_id = ?1
         LEFT JOIN ml_jobs j
           ON j.asset_id = a.id AND j.kind = ?2
         WHERE a.deleted_at IS NULL
           AND a.media_type = 'image'
           AND e.asset_id IS NULL
           AND (j.state IS NULL OR (j.state = 'failed' AND j.attempts < 3))
         ORDER BY a.indexed_at DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![IMAGE_MODEL_ID, ModelKind::ClipImage.as_str(), limit],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// How much of the library has been embedded.
pub fn coverage(conn: &Connection) -> AppResult<(i64, i64)> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type = 'image'",
        [],
        |r| r.get(0),
    )?;
    let done: i64 = conn.query_row(
        "SELECT COUNT(*) FROM asset_embeddings e
         JOIN assets a ON a.id = e.asset_id AND a.deleted_at IS NULL
         WHERE e.model_id = ?1",
        params![IMAGE_MODEL_ID],
        |r| r.get(0),
    )?;
    Ok((done, total))
}

/// Rank the library against a query vector, best first.
///
/// Trashed assets are excluded in SQL rather than filtered afterwards, so a
/// deleted photo can never surface in results.
pub fn search_by_vector(
    conn: &Connection,
    query: &[f32],
    limit: usize,
) -> AppResult<Vec<(String, f32)>> {
    let mut q = query.to_vec();
    vector::normalize(&mut q);

    let mut stmt = conn.prepare(
        "SELECT e.asset_id, e.vector
         FROM asset_embeddings e
         JOIN assets a ON a.id = e.asset_id
         WHERE e.model_id = ?1 AND a.deleted_at IS NULL",
    )?;
    let rows = stmt.query_map(params![IMAGE_MODEL_ID], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;

    let mut scored: Vec<(String, f32)> = Vec::new();
    for row in rows {
        let (id, blob) = match row {
            Ok(v) => v,
            Err(_) => continue,
        };
        // A single unreadable row must not fail the whole search.
        let Ok(v) = vector::decode(&blob) else {
            continue;
        };
        let score = vector::similarity(&q, &v);
        if score >= MIN_SCORE {
            scored.push((id, score));
        }
    }

    Ok(vector::top_k(scored, limit))
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

    fn add_asset(conn: &Connection, id: &str, media_type: &str) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES (?1, ?2, ?3, ?4, '2026-01-01', '2026-01-01')",
            params![id, format!("/m/{id}.jpg"), format!("h-{id}"), media_type],
        )
        .unwrap();
    }

    /// A vector pointing mostly along one axis, so tests can reason about
    /// similarity without depending on a real model.
    fn axis(dim: usize, index: usize) -> Vec<f32> {
        let mut v = vec![0.0; dim];
        v[index] = 1.0;
        v
    }

    #[test]
    fn stored_vectors_are_normalised_so_similarity_is_a_dot_product() {
        let (_dir, conn) = open();
        add_asset(&conn, "a1", "image");
        // Deliberately not unit length.
        store(&conn, "a1", IMAGE_MODEL_ID, &[3.0, 4.0]).unwrap();

        let blob: Vec<u8> = conn
            .query_row(
                "SELECT vector FROM asset_embeddings WHERE asset_id = 'a1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v = vector::decode(&blob).unwrap();
        let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-5, "stored length {len}");
    }

    #[test]
    fn search_ranks_the_closest_vector_first() {
        let (_dir, conn) = open();
        for id in ["a1", "a2", "a3"] {
            add_asset(&conn, id, "image");
        }
        store(&conn, "a1", IMAGE_MODEL_ID, &axis(4, 0)).unwrap();
        store(&conn, "a2", IMAGE_MODEL_ID, &axis(4, 1)).unwrap();
        // Halfway between axis 0 and axis 1.
        store(&conn, "a3", IMAGE_MODEL_ID, &[0.7, 0.7, 0.0, 0.0]).unwrap();

        let hits = search_by_vector(&conn, &axis(4, 0), 10).unwrap();
        assert_eq!(hits[0].0, "a1", "exact direction must rank first");
        assert_eq!(hits[1].0, "a3", "partial match ranks second");
        assert!(
            hits.iter().all(|(id, _)| id != "a2"),
            "orthogonal vector scores 0 and is below the floor"
        );
    }

    #[test]
    fn search_respects_the_limit() {
        let (_dir, conn) = open();
        for i in 0..10 {
            let id = format!("a{i}");
            add_asset(&conn, &id, "image");
            store(&conn, &id, IMAGE_MODEL_ID, &axis(4, 0)).unwrap();
        }
        assert_eq!(search_by_vector(&conn, &axis(4, 0), 3).unwrap().len(), 3);
    }

    #[test]
    fn search_never_returns_trashed_assets() {
        let (_dir, conn) = open();
        add_asset(&conn, "a1", "image");
        add_asset(&conn, "a2", "image");
        store(&conn, "a1", IMAGE_MODEL_ID, &axis(4, 0)).unwrap();
        store(&conn, "a2", IMAGE_MODEL_ID, &axis(4, 0)).unwrap();
        conn.execute(
            "UPDATE assets SET deleted_at = '2026-07-01' WHERE id = 'a2'",
            [],
        )
        .unwrap();

        let hits = search_by_vector(&conn, &axis(4, 0), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "a1");
    }

    #[test]
    fn search_survives_a_corrupt_embedding_row() {
        let (_dir, conn) = open();
        add_asset(&conn, "good", "image");
        add_asset(&conn, "bad", "image");
        store(&conn, "good", IMAGE_MODEL_ID, &axis(4, 0)).unwrap();
        // Odd byte count can't be whole f32s.
        conn.execute(
            "INSERT INTO asset_embeddings (asset_id, model_id, dim, vector, created_at)
             VALUES ('bad', ?1, 4, X'ABCDEF', '2026-01-01')",
            params![IMAGE_MODEL_ID],
        )
        .unwrap();

        let hits = search_by_vector(&conn, &axis(4, 0), 10).unwrap();
        assert_eq!(hits.len(), 1, "one bad row must not fail the search");
        assert_eq!(hits[0].0, "good");
    }

    #[test]
    fn search_on_an_unembedded_library_returns_nothing_rather_than_erroring() {
        let (_dir, conn) = open();
        add_asset(&conn, "a1", "image");
        assert!(search_by_vector(&conn, &axis(4, 0), 10).unwrap().is_empty());
    }

    #[test]
    fn pending_skips_embedded_videos_and_trashed_assets() {
        let (_dir, conn) = open();
        add_asset(&conn, "todo", "image");
        add_asset(&conn, "done", "image");
        add_asset(&conn, "clip", "video");
        add_asset(&conn, "gone", "image");
        store(&conn, "done", IMAGE_MODEL_ID, &axis(4, 0)).unwrap();
        conn.execute(
            "UPDATE assets SET deleted_at = '2026-07-01' WHERE id = 'gone'",
            [],
        )
        .unwrap();

        let pending: Vec<String> = pending_assets(&conn, 50)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(pending, ["todo"], "got {pending:?}");
    }

    #[test]
    fn a_repeatedly_failing_asset_stops_being_retried() {
        let (_dir, conn) = open();
        add_asset(&conn, "broken", "image");

        for _ in 0..3 {
            assert_eq!(pending_assets(&conn, 50).unwrap().len(), 1);
            mark_job(&conn, "broken", ModelKind::ClipImage, "failed", Some("boom")).unwrap();
        }

        assert!(
            pending_assets(&conn, 50).unwrap().is_empty(),
            "must give up after 3 attempts instead of looping forever"
        );
    }

    #[test]
    fn coverage_counts_only_live_images() {
        let (_dir, conn) = open();
        add_asset(&conn, "a1", "image");
        add_asset(&conn, "a2", "image");
        add_asset(&conn, "clip", "video");
        store(&conn, "a1", IMAGE_MODEL_ID, &axis(4, 0)).unwrap();

        assert_eq!(coverage(&conn).unwrap(), (1, 2));
    }

    #[test]
    fn model_paths_reports_the_first_missing_file() {
        let (_dir, conn) = open();
        let err = model_paths(&conn).unwrap_err().to_string();
        assert!(err.contains(IMAGE_MODEL_ID), "{err}");
        assert!(err.contains("not installed"), "{err}");
    }

    #[test]
    fn re_embedding_an_asset_replaces_rather_than_duplicates() {
        let (_dir, conn) = open();
        add_asset(&conn, "a1", "image");
        store(&conn, "a1", IMAGE_MODEL_ID, &axis(4, 0)).unwrap();
        store(&conn, "a1", IMAGE_MODEL_ID, &axis(4, 1)).unwrap();

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM asset_embeddings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);

        let hits = search_by_vector(&conn, &axis(4, 1), 10).unwrap();
        assert_eq!(hits[0].0, "a1", "latest vector wins");
    }
}
