//! Online nearest-centroid face clustering.

use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::indexer;
use crate::ml::vector;

/// Cosine similarity threshold for joining an existing person.
///
/// ArcFace embeddings of the same person typically land well above 0.35 once
/// L2-normalised. The previous 0.42 bar over-split: slight pose/lighting
/// changes created a new cluster instead of joining the right one.
pub const MATCH_THRESHOLD: f32 = 0.32;

/// When two people centroids are at least this similar, consolidate them.
/// Runs after detection batches so early over-splits heal without a full recluster.
pub const CONSOLIDATE_THRESHOLD: f32 = 0.40;

#[derive(Debug, Clone)]
pub struct AssignResult {
    pub person_id: String,
    pub created: bool,
}

/// Assign `embedding` to the nearest person above [`MATCH_THRESHOLD`], or create
/// a new unnamed person. Updates the running centroid.
pub fn assign(conn: &Connection, embedding: &[f32]) -> AppResult<AssignResult> {
    let mut best_id: Option<String> = None;
    let mut best_sim = f32::NEG_INFINITY;

    {
        let mut stmt = conn.prepare(
            "SELECT id, centroid FROM people WHERE centroid IS NOT NULL AND centroid_count > 0",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (id, blob) = row?;
            let centroid = vector::decode(&blob)?;
            let sim = vector::similarity(embedding, &centroid);
            if sim > best_sim {
                best_sim = sim;
                best_id = Some(id);
            }
        }
    }

    if let Some(id) = best_id.filter(|_| best_sim >= MATCH_THRESHOLD) {
        update_centroid(conn, &id, embedding)?;
        return Ok(AssignResult {
            person_id: id,
            created: false,
        });
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let mut centroid = embedding.to_vec();
    vector::normalize(&mut centroid);
    conn.execute(
        "INSERT INTO people (id, name, cover_face_id, face_count, centroid, centroid_count, created_at, updated_at)
         VALUES (?1, NULL, NULL, 0, ?2, 1, ?3, ?3)",
        params![id, vector::encode(&centroid), now],
    )?;
    Ok(AssignResult {
        person_id: id,
        created: true,
    })
}

fn update_centroid(conn: &Connection, person_id: &str, embedding: &[f32]) -> AppResult<()> {
    let (blob, count): (Vec<u8>, i64) = conn.query_row(
        "SELECT centroid, centroid_count FROM people WHERE id = ?1",
        params![person_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let mut centroid = vector::decode(&blob)?;
    let n = count.max(0) as f32;
    if centroid.len() != embedding.len() {
        centroid = embedding.to_vec();
    } else {
        for (c, e) in centroid.iter_mut().zip(embedding.iter()) {
            *c = (*c * n + *e) / (n + 1.0);
        }
    }
    vector::normalize(&mut centroid);
    conn.execute(
        "UPDATE people SET centroid = ?1, centroid_count = centroid_count + 1, updated_at = ?2
         WHERE id = ?3",
        params![
            vector::encode(&centroid),
            chrono::Utc::now().to_rfc3339(),
            person_id
        ],
    )?;
    Ok(())
}

/// Merge `from_id` into `into_id`. Named `into_id` keeps its name; faces move over.
pub fn merge(conn: &Connection, into_id: &str, from_id: &str) -> AppResult<()> {
    if into_id == from_id {
        return Ok(());
    }
    let asset_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT asset_id FROM faces WHERE person_id IN (?1, ?2)",
        )?;
        let rows = stmt.query_map(params![into_id, from_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    conn.execute(
        "UPDATE faces SET person_id = ?1 WHERE person_id = ?2",
        params![into_id, from_id],
    )?;

    // Rebuild centroid of the surviving person from all its faces.
    rebuild_centroid(conn, into_id)?;
    refresh_person_stats(conn, into_id)?;

    conn.execute("DELETE FROM people WHERE id = ?1", params![from_id])?;

    for asset_id in &asset_ids {
        let _ = indexer::refresh_fts(conn, asset_id);
    }
    Ok(())
}

/// Detach a face from its person (creates a new unnamed person for it).
pub fn detach(conn: &Connection, face_id: &str) -> AppResult<String> {
    let (old_person, embedding, asset_id): (Option<String>, Vec<u8>, String) = conn.query_row(
        "SELECT person_id, embedding, asset_id FROM faces WHERE id = ?1",
        params![face_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let emb = vector::decode(&embedding)?;
    let new_person = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let mut centroid = emb.clone();
    vector::normalize(&mut centroid);
    conn.execute(
        "INSERT INTO people (id, name, cover_face_id, face_count, centroid, centroid_count, created_at, updated_at)
         VALUES (?1, NULL, ?2, 1, ?3, 1, ?4, ?4)",
        params![new_person, face_id, vector::encode(&centroid), now],
    )?;
    conn.execute(
        "UPDATE faces SET person_id = ?1 WHERE id = ?2",
        params![new_person, face_id],
    )?;

    if let Some(old) = old_person {
        let remaining: i64 = conn.query_row(
            "SELECT COUNT(*) FROM faces WHERE person_id = ?1",
            params![old],
            |r| r.get(0),
        )?;
        if remaining == 0 {
            conn.execute(
                "DELETE FROM people WHERE id = ?1 AND ignored = 0",
                params![old],
            )?;
            refresh_person_stats(conn, &old)?;
        } else {
            rebuild_centroid(conn, &old)?;
            refresh_person_stats(conn, &old)?;
        }
    }

    let _ = indexer::refresh_fts(conn, &asset_id);
    Ok(new_person)
}

/// Reassign every face belonging to an *unnamed* person. Named people are sticky,
/// and so are ignored ones — reclustering must not resurface a hidden face.
pub fn recluster_unnamed(conn: &Connection) -> AppResult<usize> {
    // Collect faces on unnamed people.
    let faces: Vec<(String, Vec<u8>, String)> = {
        let mut stmt = conn.prepare(
            "SELECT f.id, f.embedding, f.asset_id
             FROM faces f
             JOIN people p ON p.id = f.person_id
             WHERE p.ignored = 0 AND (p.name IS NULL OR TRIM(p.name) = '')",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Detach them all into a temporary pool by clearing person_id, then delete empty people.
    for (face_id, _, _) in &faces {
        conn.execute(
            "UPDATE faces SET person_id = NULL WHERE id = ?1",
            params![face_id],
        )?;
    }
    conn.execute(
        "DELETE FROM people WHERE ignored = 0 AND (name IS NULL OR TRIM(name) = '')
         AND id NOT IN (SELECT person_id FROM faces WHERE person_id IS NOT NULL)",
        [],
    )?;

    let mut n = 0usize;
    let mut touched = std::collections::HashSet::new();
    for (face_id, blob, asset_id) in faces {
        let emb = vector::decode(&blob)?;
        let assigned = assign(conn, &emb)?;
        conn.execute(
            "UPDATE faces SET person_id = ?1 WHERE id = ?2",
            params![assigned.person_id, face_id],
        )?;
        refresh_person_stats(conn, &assigned.person_id)?;
        touched.insert(asset_id);
        n += 1;
    }
    // Heal residual over-splits between the newly rebuilt unnamed clusters.
    let _ = consolidate_similar_people(conn)?;
    for asset_id in touched {
        let _ = indexer::refresh_fts(conn, &asset_id);
    }
    Ok(n)
}

/// Merge people whose centroids are very similar. Named people stay sticky:
/// faces move into the named id. Two differently named people are never merged.
pub fn consolidate_similar_people(conn: &Connection) -> AppResult<usize> {
    let people: Vec<(String, Option<String>, Vec<u8>)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, centroid FROM people
             WHERE ignored = 0 AND centroid IS NOT NULL AND centroid_count > 0
               AND face_count > 0",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut merged = 0usize;
    let mut absorbed: std::collections::HashSet<String> = std::collections::HashSet::new();

    for i in 0..people.len() {
        if absorbed.contains(&people[i].0) {
            continue;
        }
        let emb_i = match vector::decode(&people[i].2) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for j in (i + 1)..people.len() {
            if absorbed.contains(&people[j].0) {
                continue;
            }
            let emb_j = match vector::decode(&people[j].2) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if vector::similarity(&emb_i, &emb_j) < CONSOLIDATE_THRESHOLD {
                continue;
            }

            let name_i = people[i]
                .1
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let name_j = people[j]
                .1
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());

            // Never auto-merge two differently named people.
            if name_i.is_some() && name_j.is_some() && name_i != name_j {
                continue;
            }

            let (into, from) = match (name_i, name_j) {
                (Some(_), None) => (&people[i].0, &people[j].0),
                (None, Some(_)) => (&people[j].0, &people[i].0),
                _ => {
                    let count_i: i64 = conn
                        .query_row(
                            "SELECT face_count FROM people WHERE id = ?1",
                            params![people[i].0],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let count_j: i64 = conn
                        .query_row(
                            "SELECT face_count FROM people WHERE id = ?1",
                            params![people[j].0],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    if count_j > count_i {
                        (&people[j].0, &people[i].0)
                    } else {
                        (&people[i].0, &people[j].0)
                    }
                }
            };

            if absorbed.contains(into) || absorbed.contains(from) {
                continue;
            }
            merge(conn, into, from)?;
            absorbed.insert(from.clone());
            merged += 1;
            if from == &people[i].0 {
                break;
            }
        }
    }
    Ok(merged)
}

pub fn rename(conn: &Connection, person_id: &str, name: &str) -> AppResult<()> {
    let trimmed = name.trim();
    let name_val: Option<&str> = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    conn.execute(
        "UPDATE people SET name = ?1, updated_at = ?2 WHERE id = ?3",
        params![name_val, chrono::Utc::now().to_rfc3339(), person_id],
    )?;
    let asset_ids: Vec<String> = {
        let mut stmt =
            conn.prepare("SELECT DISTINCT asset_id FROM faces WHERE person_id = ?1")?;
        let rows = stmt.query_map(params![person_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    for asset_id in asset_ids {
        let _ = indexer::refresh_fts(conn, &asset_id);
    }
    Ok(())
}

pub fn rebuild_centroid(conn: &Connection, person_id: &str) -> AppResult<()> {
    let embeddings: Vec<Vec<u8>> = {
        let mut stmt = conn.prepare("SELECT embedding FROM faces WHERE person_id = ?1")?;
        let rows = stmt.query_map(params![person_id], |r| r.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    if embeddings.is_empty() {
        // An ignored person with no faces left still needs its centroid, or the
        // same face would come back as a brand new person.
        conn.execute(
            "UPDATE people SET centroid = NULL, centroid_count = 0, updated_at = ?1
             WHERE id = ?2 AND ignored = 0",
            params![chrono::Utc::now().to_rfc3339(), person_id],
        )?;
        return Ok(());
    }
    let mut acc: Option<Vec<f32>> = None;
    let mut count = 0f32;
    for blob in &embeddings {
        let v = vector::decode(blob)?;
        match &mut acc {
            None => acc = Some(v),
            Some(a) => {
                if a.len() == v.len() {
                    for (x, y) in a.iter_mut().zip(v.iter()) {
                        *x += *y;
                    }
                }
            }
        }
        count += 1.0;
    }
    let mut centroid = acc.unwrap_or_default();
    if count > 0.0 {
        for x in &mut centroid {
            *x /= count;
        }
    }
    vector::normalize(&mut centroid);
    conn.execute(
        "UPDATE people SET centroid = ?1, centroid_count = ?2, updated_at = ?3 WHERE id = ?4",
        params![
            vector::encode(&centroid),
            count as i64,
            chrono::Utc::now().to_rfc3339(),
            person_id
        ],
    )?;
    Ok(())
}

pub fn refresh_person_stats(conn: &Connection, person_id: &str) -> AppResult<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM faces WHERE person_id = ?1",
        params![person_id],
        |r| r.get(0),
    )?;
    // Prefer a cover whose crop JPEG still exists; fall back to highest score.
    let cover: Option<String> = {
        let mut stmt = conn.prepare(
            "SELECT id, crop_path FROM faces WHERE person_id = ?1 ORDER BY score DESC",
        )?;
        let rows = stmt.query_map(params![person_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        let mut fallback = None;
        let mut with_crop = None;
        for row in rows.flatten() {
            let (id, crop) = row;
            if fallback.is_none() {
                fallback = Some(id.clone());
            }
            if with_crop.is_none() && crop.as_deref().is_some_and(|p| Path::new(p).is_file()) {
                with_crop = Some(id);
                break;
            }
        }
        with_crop.or(fallback)
    };
    conn.execute(
        "UPDATE people SET face_count = ?1, cover_face_id = ?2, updated_at = ?3 WHERE id = ?4",
        params![
            count,
            cover,
            chrono::Utc::now().to_rfc3339(),
            person_id
        ],
    )?;
    Ok(())
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

    fn unit(i: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; 512];
        v[i] = 1.0;
        v
    }

    #[test]
    fn same_vector_joins_same_person() {
        let (_dir, conn) = open();
        let a = assign(&conn, &unit(0)).unwrap();
        let b = assign(&conn, &unit(0)).unwrap();
        assert_eq!(a.person_id, b.person_id);
        assert!(a.created);
        assert!(!b.created);
    }

    #[test]
    fn orthogonal_vectors_split() {
        let (_dir, conn) = open();
        let a = assign(&conn, &unit(0)).unwrap();
        let b = assign(&conn, &unit(1)).unwrap();
        assert_ne!(a.person_id, b.person_id);
        assert!(b.created);
    }

    #[test]
    fn rename_and_merge() {
        let (_dir, conn) = open();
        let a = assign(&conn, &unit(0)).unwrap();
        let b = assign(&conn, &unit(1)).unwrap();
        rename(&conn, &a.person_id, "Alice").unwrap();
        merge(&conn, &a.person_id, &b.person_id).unwrap();
        let name: Option<String> = conn
            .query_row(
                "SELECT name FROM people WHERE id = ?1",
                params![a.person_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name.as_deref(), Some("Alice"));
        let gone: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM people WHERE id = ?1",
                params![b.person_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gone, 0);
    }

    #[test]
    fn consolidate_merges_near_duplicate_unnamed_clusters() {
        let (_dir, conn) = open();
        let mut a = unit(0);
        let mut b = unit(0);
        b[1] = 0.05;
        vector::normalize(&mut a);
        vector::normalize(&mut b);
        let p1 = assign(&conn, &a).unwrap();
        // assign() leaves face_count at 0 until faces are stored; bump it so
        // consolidate considers the cluster.
        conn.execute(
            "UPDATE people SET face_count = 1 WHERE id = ?1",
            params![p1.person_id],
        )
        .unwrap();
        let p2 = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO people (id, name, cover_face_id, face_count, centroid, centroid_count, created_at, updated_at)
             VALUES (?1, NULL, NULL, 1, ?2, 1, ?3, ?3)",
            params![p2, vector::encode(&b), now],
        )
        .unwrap();
        let merged = consolidate_similar_people(&conn).unwrap();
        assert!(merged >= 1);
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM people", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1);
    }
}
