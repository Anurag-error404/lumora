use rusqlite::Connection;

use crate::error::AppResult;
use crate::models::DuplicateGroup;

/// Max Hamming distance (aHash bits) to treat two images as near-duplicates.
pub const NEAR_DUP_HAMMING_THRESHOLD: u32 = 5;

pub fn find_exact_duplicates(conn: &Connection) -> AppResult<Vec<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT hash, GROUP_CONCAT(id) FROM assets
         WHERE deleted_at IS NULL
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
    u64::from_str_radix(s, 16).ok()
}

fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Cluster images whose perceptual hashes are within [`NEAR_DUP_HAMMING_THRESHOLD`].
/// Exact SHA duplicates are left to [`find_exact_duplicates`]; near groups prefer
/// pairs with different content hashes when available.
pub fn find_near_duplicates(conn: &Connection) -> AppResult<Vec<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT id, hash, perceptual_hash FROM assets
         WHERE deleted_at IS NULL
           AND perceptual_hash IS NOT NULL
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
            entries.push((id, hash, bits));
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
             VALUES ('a','/a.jpg','hhh','image','t','t'),
                    ('b','/b.jpg','hhh','image','t','t'),
                    ('c','/c.jpg','zzz','image','t','t')",
            [],
        )
        .unwrap();
        let groups = find_exact_duplicates(&conn).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].asset_ids.len(), 2);
    }

    #[test]
    fn near_dupes_by_hamming_distance() {
        // Two hashes 3 bits apart (< threshold 5), different SHA.
        let a = 0u64;
        let b = 0b111u64; // hamming 3
        let c = !0u64; // far from a/b
        let entries = vec![
            ("a".into(), "sha-a".into(), a),
            ("b".into(), "sha-b".into(), b),
            ("c".into(), "sha-c".into(), c),
        ];
        let groups = cluster_near_duplicates(&entries, 5);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].asset_ids.len(), 2);
        assert!(groups[0].asset_ids.contains(&"a".into()));
        assert!(groups[0].asset_ids.contains(&"b".into()));
    }

    #[test]
    fn near_dupes_skip_same_sha() {
        let entries = vec![
            ("a".into(), "same".into(), 0u64),
            ("b".into(), "same".into(), 0u64),
        ];
        let groups = cluster_near_duplicates(&entries, 5);
        assert!(groups.is_empty());
    }

    #[test]
    fn near_dupes_from_db() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&path).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, perceptual_hash, media_type, created_at, indexed_at)
             VALUES ('a','/a.jpg','sha-a','0000000000000000','image','t','t'),
                    ('b','/b.jpg','sha-b','0000000000000007','image','t','t'),
                    ('c','/c.jpg','sha-c','ffffffffffffffff','image','t','t')",
            [],
        )
        .unwrap();
        let groups = find_near_duplicates(&conn).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].asset_ids.len(), 2);
    }
}
