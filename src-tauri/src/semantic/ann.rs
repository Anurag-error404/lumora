//! HNSW approximate nearest-neighbour index for CLIP semantic search.
//!
//! SQLite remains the source of truth for embeddings; this layer is derived
//! data that accelerates `search_by_vector` on larger libraries. When the index
//! is missing, stale, or too small to benefit, search falls back to brute force.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use hnsw_rs::anndists::dist::distances::DistCosine;
use hnsw_rs::hnsw::{Hnsw, Neighbour};
use parking_lot::Mutex;
use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::ml::vector;

use super::{IMAGE_MODEL_ID, MIN_SCORE};

/// Below this count brute force is typically faster than index overhead.
pub const MIN_VECTORS_FOR_ANN: usize = 1_000;
const MAX_LAYER: usize = 16;
const MAX_EDGES: usize = 16;
const EF_CONSTRUCT: usize = 200;

/// Search width — must exceed requested *k*; higher = better recall, slower.
fn ef_search(limit: usize) -> usize {
    limit.saturating_mul(20).clamp(64, 512)
}

struct AnnIndex {
    hnsw: Hnsw<'static, f32, DistCosine>,
    id_to_asset: Vec<String>,
}

static CACHE: OnceLock<Mutex<Option<AnnIndex>>> = OnceLock::new();
static DIRTY: AtomicBool = AtomicBool::new(true);

fn cache() -> &'static Mutex<Option<AnnIndex>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Try to ensure the HNSW index is warm. Safe to call from a background
/// worker after an embed batch — no-op when the library is too small.
pub fn warm(conn: &Connection) -> AppResult<()> {
    let _ = ensure_loaded(conn)?;
    Ok(())
}

pub fn invalidate() {
    DIRTY.store(true, Ordering::Release);
    *cache().lock() = None;
}

/// Free the in-memory HNSW without forcing a rebuild on next search.
pub fn drop_resident() {
    *cache().lock() = None;
}

pub fn invalidate_and_remove(_app_data: &std::path::Path) {
    invalidate();
}

pub fn mark_dirty() {
    DIRTY.store(true, Ordering::Release);
}

fn embedding_count(conn: &Connection) -> AppResult<usize> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM asset_embeddings e
         JOIN assets a ON a.id = e.asset_id AND a.deleted_at IS NULL
         WHERE e.model_id = ?1",
        params![IMAGE_MODEL_ID],
        |r| r.get(0),
    )?;
    Ok(n.max(0) as usize)
}

fn load_vectors(conn: &Connection) -> AppResult<Vec<(String, Vec<f32>)>> {
    let mut stmt = conn.prepare(
        "SELECT e.asset_id, e.vector
         FROM asset_embeddings e
         JOIN assets a ON a.id = e.asset_id
         WHERE e.model_id = ?1 AND a.deleted_at IS NULL
         ORDER BY e.asset_id ASC",
    )?;
    let rows = stmt.query_map(params![IMAGE_MODEL_ID], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, blob) = row?;
        let Ok(v) = vector::decode(&blob) else {
            continue;
        };
        out.push((id, v));
    }
    Ok(out)
}

fn cosine_similarity_from_distance(distance: f32) -> f32 {
    // anndists cosine distance for unit vectors: 1 - dot(a, b)
    1.0 - distance
}

fn neighbours_to_hits(neighbours: Vec<Neighbour>, id_to_asset: &[String]) -> Vec<(String, f32)> {
    let mut out = Vec::with_capacity(neighbours.len());
    for n in neighbours {
        let Some(asset_id) = id_to_asset.get(n.d_id).cloned() else {
            continue;
        };
        let score = cosine_similarity_from_distance(n.distance);
        if score >= MIN_SCORE {
            out.push((asset_id, score));
        }
    }
    out
}

fn build_index(conn: &Connection) -> AppResult<AnnIndex> {
    let vectors = load_vectors(conn)?;
    if vectors.len() < MIN_VECTORS_FOR_ANN {
        return Err(AppError::msg("library too small for ANN index"));
    }
    let id_to_asset: Vec<String> = vectors.iter().map(|(id, _)| id.clone()).collect();
    let vecs: Vec<Vec<f32>> = vectors.into_iter().map(|(_, v)| v).collect();
    let nb = vecs.len();
    let hnsw = Hnsw::new(MAX_EDGES, nb, MAX_LAYER, EF_CONSTRUCT, DistCosine {});
    let batch: Vec<(&Vec<f32>, usize)> = vecs.iter().enumerate().map(|(i, v)| (v, i)).collect();
    hnsw.parallel_insert(&batch);
    Ok(AnnIndex { hnsw, id_to_asset })
}

fn ensure_loaded(conn: &Connection) -> AppResult<Option<()>> {
    if embedding_count(conn)? < MIN_VECTORS_FOR_ANN {
        return Ok(None);
    }

    {
        let guard = cache().lock();
        if guard.is_some() && !DIRTY.load(Ordering::Acquire) {
            return Ok(Some(()));
        }
    }

    let index = build_index(conn)?;
    *cache().lock() = Some(index);
    DIRTY.store(false, Ordering::Release);
    Ok(Some(()))
}

/// Try ANN search. Returns `None` when the index is unavailable (caller should
/// brute force).
pub fn try_search(
    conn: &Connection,
    _app_data: &std::path::Path,
    query: &[f32],
    limit: usize,
) -> AppResult<Option<Vec<(String, f32)>>> {
    if limit == 0 {
        return Ok(Some(Vec::new()));
    }
    if embedding_count(conn)? < MIN_VECTORS_FOR_ANN {
        return Ok(None);
    }

    ensure_loaded(conn)?;

    let guard = cache().lock();
    let Some(index) = guard.as_ref() else {
        return Ok(None);
    };

    let mut q = query.to_vec();
    vector::normalize(&mut q);

    let ef = ef_search(limit);
    let knbn = limit.saturating_mul(4).min(index.id_to_asset.len()).max(limit);
    let neighbours = index.hnsw.search(&q, knbn, ef);
    let hits = neighbours_to_hits(neighbours, &index.id_to_asset);
    Ok(Some(vector::top_k(hits, limit)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::sync::OnceLock;
    use tempfile::tempdir;

    /// ANN tests share a process-wide index cache; run them serially.
    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn serial() -> parking_lot::MutexGuard<'static, ()> {
        TEST_LOCK.get_or_init(|| Mutex::new(())).lock()
    }

    fn open() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        (dir, conn)
    }

    fn add_asset(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES (?1, ?2, ?3, 'image', '2026-01-01', '2026-01-01')",
            params![id, format!("/m/{id}.jpg"), format!("h-{id}")],
        )
        .unwrap();
    }

    fn unique_unit_vector(dim: usize, seed: usize) -> Vec<f32> {
        let mut v: Vec<f32> = (0..dim)
            .map(|j| ((seed.wrapping_mul(7919) + j.wrapping_mul(997)) as f32).sin())
            .collect();
        vector::normalize(&mut v);
        v
    }

    fn seed_many(conn: &Connection, n: usize, dim: usize) {
        for i in 0..n {
            let id = format!("a{i:06}");
            add_asset(conn, &id);
            let v = unique_unit_vector(dim, i);
            super::super::store(conn, &id, IMAGE_MODEL_ID, &v).unwrap();
        }
    }

    #[test]
    fn ann_skips_tiny_libraries() {
        let _lock = serial();
        invalidate();
        let (dir, conn) = open();
        add_asset(&conn, "a0");
        super::super::store(&conn, "a0", IMAGE_MODEL_ID, &unique_unit_vector(4, 0)).unwrap();
        assert!(try_search(&conn, dir.path(), &unique_unit_vector(4, 0), 5)
            .unwrap()
            .is_none());
    }

    #[test]
    fn ann_finds_nearest_among_many() {
        let _lock = serial();
        invalidate();
        let (dir, conn) = open();
        const DIM: usize = 512;
        let n = MIN_VECTORS_FOR_ANN + 50;
        seed_many(&conn, n, DIM);
        invalidate();
        let target_id = "a000042";
        let q = unique_unit_vector(DIM, 42);
        let mut q_norm = q.clone();
        vector::normalize(&mut q_norm);
        let brute = super::super::search_by_vector_brute(&conn, &q_norm, 5).unwrap();
        assert_eq!(brute[0].0, target_id, "brute force baseline");
        let hits = try_search(&conn, dir.path(), &q, 5)
            .unwrap()
            .expect("ann should run");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].0, target_id);
        assert!(hits[0].1 > 0.99, "score {}", hits[0].1);
    }

    #[test]
    fn ann_rebuilds_after_mark_dirty() {
        let _lock = serial();
        invalidate();
        let (dir, conn) = open();
        const DIM: usize = 512;
        let n = MIN_VECTORS_FOR_ANN + 10;
        seed_many(&conn, n, DIM);
        invalidate();
        assert!(try_search(&conn, dir.path(), &unique_unit_vector(DIM, 0), 3)
            .unwrap()
            .is_some());
        mark_dirty();
        assert!(try_search(&conn, dir.path(), &unique_unit_vector(DIM, 1), 3)
            .unwrap()
            .is_some());
    }
}
