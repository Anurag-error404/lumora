use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::blur;
use crate::error::AppResult;
use crate::history;
use crate::indexer::{self, MediaKind};
use crate::models::{DuplicateGroup, DuplicateScanProgress, DuplicateScanResult};
use crate::thumbnails;
use crate::watcher;

/// Max Hamming distance (aHash bits) to treat two images as near-duplicates.
/// Kept tight (≤2 of 64) to limit false positives from coarse 8×8 aHash.
pub const NEAR_DUP_HAMMING_THRESHOLD: u32 = 2;

pub fn find_exact_duplicates(conn: &Connection) -> AppResult<Vec<DuplicateGroup>> {
    // Only real SHA-256 digests. Empty/placeholder hashes must never form a group.
    // Subquery ORDER BY makes GROUP_CONCAT member order stable (oldest first) so
    // "keep first" in the UI is deterministic.
    let mut stmt = conn.prepare(
        "SELECT hash, GROUP_CONCAT(id) FROM (
            SELECT hash, id FROM assets
             WHERE deleted_at IS NULL
               AND hash IS NOT NULL
               AND length(hash) = 64
             ORDER BY hash,
                      COALESCE(captured_at, created_at) ASC,
                      path ASC
         )
         GROUP BY hash
         HAVING COUNT(*) > 1",
    )?;
    let rows = stmt.query_map([], |row| {
        let hash: String = row.get(0)?;
        let ids: String = row.get(1)?;
        Ok(DuplicateGroup {
            kind: "exact".into(),
            key: hash,
            asset_ids: ids.split(',').map(|s| s.to_string()).collect(),
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn parse_phash(s: &str) -> Option<u64> {
    if s.len() != 16 {
        return None;
    }
    u64::from_str_radix(s, 16).ok()
}

fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Reject near-solid aHashes (all black / all white / decode junk). Those collide
/// across unrelated photos and create huge false near-dupe groups.
fn phash_is_discriminative(bits: u64) -> bool {
    let ones = bits.count_ones();
    (4..=60).contains(&ones)
}

/// Cluster images whose perceptual hashes are within [`NEAR_DUP_HAMMING_THRESHOLD`].
/// Exact SHA duplicates are left to [`find_exact_duplicates`]; near groups prefer
/// pairs with different content hashes when available.
pub fn find_near_duplicates(conn: &Connection) -> AppResult<Vec<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT id, hash, perceptual_hash FROM assets
         WHERE deleted_at IS NULL
           AND perceptual_hash IS NOT NULL
           AND length(hash) = 64
           AND media_type = 'image'",
    )?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let hash: String = row.get(1)?;
        let ph: String = row.get(2)?;
        Ok((id, hash, ph))
    })?;

    let mut entries: Vec<(String, String, u64)> = Vec::new();
    for row in rows {
        let (id, hash, ph) = row?;
        if let Some(bits) = parse_phash(&ph) {
            if phash_is_discriminative(bits) {
                entries.push((id, hash, bits));
            }
        }
    }

    Ok(cluster_near_duplicates(
        &entries,
        NEAR_DUP_HAMMING_THRESHOLD,
    ))
}

fn cluster_near_duplicates(
    entries: &[(String, String, u64)],
    threshold: u32,
) -> Vec<DuplicateGroup> {
    let n = entries.len();
    if n < 2 {
        return Vec::new();
    }

    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank = vec![0u8; n];

    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    fn union(parent: &mut [usize], rank: &mut [u8], a: usize, b: usize) {
        let mut ra = find(parent, a);
        let mut rb = find(parent, b);
        if ra == rb {
            return;
        }
        if rank[ra] < rank[rb] {
            std::mem::swap(&mut ra, &mut rb);
        }
        parent[rb] = ra;
        if rank[ra] == rank[rb] {
            rank[ra] += 1;
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            // Skip identical-file pairs — those belong in exact-dupe groups.
            if entries[i].1 == entries[j].1 {
                continue;
            }
            if hamming(entries[i].2, entries[j].2) <= threshold {
                union(&mut parent, &mut rank, i, j);
            }
        }
    }

    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        buckets[find(&mut parent, i)].push(i);
    }

    let mut groups = Vec::new();
    for members in buckets {
        if members.len() < 2 {
            continue;
        }
        let key = format!("{:016x}", entries[members[0]].2);
        groups.push(DuplicateGroup {
            kind: "near".into(),
            key,
            asset_ids: members.into_iter().map(|i| entries[i].0.clone()).collect(),
        });
    }
    groups
}

pub fn all_duplicates(conn: &Connection) -> AppResult<Vec<DuplicateGroup>> {
    let mut groups = find_exact_duplicates(conn)?;
    groups.extend(find_near_duplicates(conn)?);
    Ok(groups)
}

/// Manual scan: index previously skipped on-disk copies, backfill phash/blur, regroup.
pub fn scan_duplicates(conn: &Connection, limit: u32) -> AppResult<DuplicateScanResult> {
    scan_duplicates_with_progress(conn, limit, &[], |_| {})
}

/// Same as [`scan_duplicates`], emitting progress through each phase.
pub fn scan_duplicates_with_progress(
    conn: &Connection,
    limit: u32,
    ignore_patterns: &[String],
    mut on_progress: impl FnMut(DuplicateScanProgress),
) -> AppResult<DuplicateScanResult> {
    let limit = limit.clamp(1, 5_000);

    on_progress(DuplicateScanProgress {
        phase: "copies".into(),
        current: 0,
        total: 0,
        path: None,
    });
    let copies_indexed =
        index_skipped_content_copies(conn, ignore_patterns, |current, total, path| {
            on_progress(DuplicateScanProgress {
                phase: "copies".into(),
                current,
                total,
                path: Some(path.to_string()),
            });
        })?;

    on_progress(DuplicateScanProgress {
        phase: "phash".into(),
        current: 0,
        total: 0,
        path: None,
    });
    let phash_backfilled = backfill_missing_phashes(conn, limit, |current, total, path| {
        on_progress(DuplicateScanProgress {
            phase: "phash".into(),
            current,
            total,
            path: Some(path.to_string()),
        });
    })?;

    on_progress(DuplicateScanProgress {
        phase: "blur".into(),
        current: 0,
        total: 0,
        path: None,
    });
    let blur_scored =
        blur::backfill_missing_with_progress(conn, limit, |current, total, path| {
            on_progress(DuplicateScanProgress {
                phase: "blur".into(),
                current,
                total,
                path: Some(path.to_string()),
            });
        })?;

    on_progress(DuplicateScanProgress {
        phase: "grouping".into(),
        current: 0,
        total: 1,
        path: None,
    });
    let groups = all_duplicates(conn)?;
    let exact_groups = groups.iter().filter(|g| g.kind == "exact").count() as u32;
    let near_groups = groups.iter().filter(|g| g.kind == "near").count() as u32;

    on_progress(DuplicateScanProgress {
        phase: "done".into(),
        current: 1,
        total: 1,
        path: None,
    });

    Ok(DuplicateScanResult {
        groups,
        copies_indexed: copies_indexed as u32,
        phash_backfilled: phash_backfilled as u32,
        blur_scored: blur_scored as u32,
        exact_groups,
        near_groups,
    })
}

#[derive(Clone)]
struct TwinMeta {
    width: Option<i64>,
    height: Option<i64>,
    duration_ms: Option<i64>,
    file_size: Option<i64>,
    perceptual_hash: Option<String>,
    thumbnail_path: Option<String>,
    captured_at: Option<String>,
    camera: Option<String>,
    lens: Option<String>,
    blur_score: Option<f64>,
}

/// Walk watched / import roots for files that match an existing content hash but
/// were never indexed (typical when Skip duplicates was on during import).
fn index_skipped_content_copies(
    conn: &Connection,
    ignore_patterns: &[String],
    mut on_progress: impl FnMut(u32, u32, &str),
) -> AppResult<usize> {
    let roots = scan_roots(conn);
    if roots.is_empty() {
        return Ok(0);
    }

    let mut known_paths: HashSet<String> = HashSet::new();
    let mut twins: HashMap<String, TwinMeta> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT path, hash, width, height, duration_ms, file_size,
                    perceptual_hash, thumbnail_path, captured_at, camera, lens, blur_score
             FROM assets
             WHERE deleted_at IS NULL
               AND length(hash) = 64",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                TwinMeta {
                    width: row.get(2)?,
                    height: row.get(3)?,
                    duration_ms: row.get(4)?,
                    file_size: row.get(5)?,
                    perceptual_hash: row.get(6)?,
                    thumbnail_path: row.get(7)?,
                    captured_at: row.get(8)?,
                    camera: row.get(9)?,
                    lens: row.get(10)?,
                    blur_score: row.get(11)?,
                },
            ))
        })?;
        for row in rows {
            let (path, hash, meta) = row?;
            known_paths.insert(path);
            twins.entry(hash).or_insert(meta);
        }
    }

    let files = indexer::collect_media_files_filtered(&roots, ignore_patterns)?;
    let candidates: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| !known_paths.contains(&p.to_string_lossy().to_string()))
        .collect();
    let total = candidates.len() as u32;
    if total == 0 {
        return Ok(0);
    }

    let now = Utc::now().to_rfc3339();
    let mut inserted = 0usize;
    for (i, path) in candidates.iter().enumerate() {
        let path_str = path.to_string_lossy().to_string();
        on_progress(i as u32 + 1, total, &path_str);
        let Ok(hash) = indexer::sha256_file(path) else {
            continue;
        };
        let Some(twin) = twins.get(&hash).cloned() else {
            continue;
        };
        let Some(kind) = indexer::media_type_for_path(path) else {
            continue;
        };
        // Prefer live file size; fall back to twin.
        let file_size = std::fs::metadata(path)
            .ok()
            .map(|m| m.len() as i64)
            .or(twin.file_size);
        let created_at = file_created_at(path).unwrap_or_else(|| now.clone());
        let id = Uuid::new_v4().to_string();
        match conn.execute(
            "INSERT INTO assets (
                id, path, hash, perceptual_hash, media_type, width, height, duration_ms, file_size,
                created_at, captured_at, indexed_at, camera, lens, blur_score, thumbnail_path
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                id,
                path_str,
                hash,
                twin.perceptual_hash,
                kind.as_str(),
                twin.width,
                twin.height,
                if kind == MediaKind::Video {
                    twin.duration_ms
                } else {
                    None
                },
                file_size,
                created_at,
                twin.captured_at,
                now,
                twin.camera,
                twin.lens,
                twin.blur_score,
                twin.thumbnail_path,
            ],
        ) {
            Ok(_) => {
                inserted += 1;
                known_paths.insert(path.to_string_lossy().to_string());
            }
            Err(e) => {
                tracing::debug!(path = %path.display(), error = %e, "skipped-copy insert failed");
            }
        }
    }
    Ok(inserted)
}

fn scan_roots(conn: &Connection) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(watched) = watcher::load_watched_paths(conn) {
        roots.extend(watched);
    }
    if let Ok(runs) = history::list_import_runs(conn, 30) {
        for run in runs {
            let Some(raw) = run.roots_json else { continue };
            if let Ok(list) = serde_json::from_str::<Vec<String>>(&raw) {
                for p in list {
                    roots.push(PathBuf::from(p));
                }
            }
        }
    }
    prune_nested_roots(roots)
}

fn prune_nested_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        if kept.iter().any(|k| root.starts_with(k)) {
            continue;
        }
        kept.retain(|k| !k.starts_with(&root));
        kept.push(root);
    }
    kept
}

fn file_created_at(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dt: chrono::DateTime<Utc> = modified.into();
    Some(dt.to_rfc3339())
}

/// Compute aHash for images that still lack `perceptual_hash` (prefer thumbnails).
fn backfill_missing_phashes(
    conn: &Connection,
    limit: u32,
    mut on_progress: impl FnMut(u32, u32, &str),
) -> AppResult<usize> {
    let mut stmt = conn.prepare(
        "SELECT id, path, thumbnail_path FROM assets
         WHERE deleted_at IS NULL
           AND media_type = 'image'
           AND (perceptual_hash IS NULL OR perceptual_hash = '')
         LIMIT ?1",
    )?;
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map(params![limit], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let total = rows.len() as u32;
    let mut updated = 0usize;
    for (i, (id, path, thumb)) in rows.into_iter().enumerate() {
        on_progress(i as u32 + 1, total, &path);
        let img = match thumb.as_deref() {
            Some(t) if Path::new(t).is_file() => image::open(t).ok(),
            _ => None,
        };
        let img = match img {
            Some(img) => img,
            None => match thumbnails::open_oriented(Path::new(&path)) {
                Ok(img) => img,
                Err(e) => {
                    tracing::debug!(asset = %id, error = %e, "phash backfill skipped");
                    continue;
                }
            },
        };
        let ph = thumbnails::perceptual_hash_from_image(&img);
        conn.execute(
            "UPDATE assets SET perceptual_hash = ?1 WHERE id = ?2",
            params![ph, id],
        )?;
        updated += 1;
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::tempdir;

    #[test]
    fn exact_dupes_by_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('a','/a.jpg','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','image','t','t'),
                    ('b','/b.jpg','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','image','t','t'),
                    ('c','/c.jpg','bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','image','t','t')",
            [],
        )
        .unwrap();
        let groups = find_exact_duplicates(&conn).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].asset_ids.len(), 2);
    }

    #[test]
    fn exact_dupes_ignore_short_hashes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at)
             VALUES ('a','/a.jpg','','image','t','t'),
                    ('b','/b.jpg','','image','t','t'),
                    ('c','/c.jpg','short','image','t','t'),
                    ('d','/d.jpg','short','image','t','t')",
            [],
        )
        .unwrap();
        assert!(find_exact_duplicates(&conn).unwrap().is_empty());
    }

    #[test]
    fn exact_dupes_order_oldest_first() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        let h = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, captured_at, indexed_at)
             VALUES ('new','/new.jpg',?1,'image','t','2024-06-01','t'),
                    ('old','/old.jpg',?1,'image','t','2020-01-01','t')",
            [h],
        )
        .unwrap();
        let groups = find_exact_duplicates(&conn).unwrap();
        assert_eq!(groups[0].asset_ids, vec!["old".to_string(), "new".to_string()]);
    }

    #[test]
    fn near_dupes_by_hamming_distance() {
        // Two hashes 2 bits apart (at threshold), different SHA.
        let a = 0x00ff00ff00ff00ffu64;
        let b = 0x00ff00ff00ff00fcu64; // hamming 2
        assert!(phash_is_discriminative(a));
        assert!(phash_is_discriminative(b));
        let entries = vec![
            ("a".into(), "sha-a".into(), a),
            ("b".into(), "sha-b".into(), b),
            ("c".into(), "sha-c".into(), 0x0f0f0f0f0f0f0f0fu64), // far
        ];
        let groups = cluster_near_duplicates(&entries, NEAR_DUP_HAMMING_THRESHOLD);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].asset_ids.len(), 2);
        assert!(groups[0].asset_ids.contains(&"a".into()));
        assert!(groups[0].asset_ids.contains(&"b".into()));
    }

    #[test]
    fn near_dupes_reject_distance_above_threshold() {
        let a = 0x00ff00ff00ff00ffu64;
        let b = 0x00ff00ff00ff0000u64; // hamming 8
        let entries = vec![
            ("a".into(), "sha-a".into(), a),
            ("b".into(), "sha-b".into(), b),
        ];
        assert!(cluster_near_duplicates(&entries, NEAR_DUP_HAMMING_THRESHOLD).is_empty());
    }

    #[test]
    fn near_dupes_skip_same_sha() {
        let bits = 0x00ff00ff00ff00ffu64;
        let entries = vec![
            ("a".into(), "same".into(), bits),
            ("b".into(), "same".into(), bits),
        ];
        let groups = cluster_near_duplicates(&entries, 5);
        assert!(groups.is_empty());
    }

    #[test]
    fn solid_phash_is_not_discriminative() {
        assert!(!phash_is_discriminative(0));
        assert!(!phash_is_discriminative(!0));
        assert!(!phash_is_discriminative(0x7)); // 3 ones — below floor
        assert!(phash_is_discriminative(0x00ff00ff00ff00ff));
    }

    #[test]
    fn near_dupes_from_db() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        let ha = "a".repeat(64);
        let hb = "b".repeat(64);
        let hc = "c".repeat(64);
        let hd = "d".repeat(64);
        conn.execute(
            "INSERT INTO assets (id, path, hash, perceptual_hash, media_type, created_at, indexed_at)
             VALUES ('a','/a.jpg',?1,'00ff00ff00ff00ff','image','t','t'),
                    ('b','/b.jpg',?2,'00ff00ff00ff00fc','image','t','t'),
                    ('c','/c.jpg',?3,'ffffffffffffffff','image','t','t'),
                    ('d','/d.jpg',?4,'ffffffffffffffff','image','t','t')",
            rusqlite::params![ha, hb, hc, hd],
        )
        .unwrap();
        let groups = find_near_duplicates(&conn).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].asset_ids.len(), 2);
        assert!(groups[0].asset_ids.contains(&"a".into()));
        assert!(groups[0].asset_ids.contains(&"b".into()));
    }
}
