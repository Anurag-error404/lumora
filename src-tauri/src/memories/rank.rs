//! Memories v1.5 ranking: base quality score + CLIP diversity when embeddings exist.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::ml::vector;
use crate::semantic::IMAGE_MODEL_ID;

/// How hard diversity pushes against near-duplicates (0 = ignore CLIP).
pub const DIVERSITY_LAMBDA: f32 = 0.65;
/// Soft floor: assets this similar to an already-picked one are heavily penalised
/// (must outweigh a favourite's +10 base so near-dups don't crowd the set).
pub const NEAR_DUP_SIM: f32 = 0.92;
pub const NEAR_DUP_PENALTY: f32 = 12.0;
/// Cap candidates loaded for re-ranking (keeps list_memories snappy).
pub const MAX_CANDIDATES: usize = 400;

#[derive(Debug, Clone)]
pub struct RankCandidate {
    pub id: String,
    pub favorite: bool,
    pub rating: i64,
    pub has_thumb: bool,
    pub embedding: Option<Vec<f32>>,
}

pub fn base_score(c: &RankCandidate) -> f32 {
    let mut s = 0.0;
    if c.favorite {
        s += 10.0;
    }
    s += (c.rating as f32) * 2.0;
    if c.has_thumb {
        s += 1.0;
    }
    s
}

/// Greedy diverse ordering. When no embeddings are present, sorts by base score.
pub fn diversify(mut candidates: Vec<RankCandidate>, limit: usize) -> Vec<String> {
    if limit == 0 || candidates.is_empty() {
        return Vec::new();
    }
    let has_any_embed = candidates.iter().any(|c| c.embedding.is_some());
    if !has_any_embed {
        candidates.sort_by(|a, b| {
            base_score(b)
                .partial_cmp(&base_score(a))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        return candidates.into_iter().take(limit).map(|c| c.id).collect();
    }

    let mut selected: Vec<RankCandidate> = Vec::with_capacity(limit.min(candidates.len()));
    let mut remaining = candidates;

    while selected.len() < limit && !remaining.is_empty() {
        let mut best_idx = 0usize;
        let mut best_score = f32::NEG_INFINITY;
        for (i, cand) in remaining.iter().enumerate() {
            let mut score = base_score(cand);
            if let Some(ref emb) = cand.embedding {
                let mut max_sim = 0.0f32;
                for picked in &selected {
                    if let Some(ref pemb) = picked.embedding {
                        max_sim = max_sim.max(vector::similarity(emb, pemb));
                    }
                }
                score -= DIVERSITY_LAMBDA * max_sim;
                if max_sim >= NEAR_DUP_SIM {
                    score -= NEAR_DUP_PENALTY;
                }
            } else {
                // Prefer embedded assets slightly when mixing, so diversity can work.
                score -= 0.5;
            }
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }
        selected.push(remaining.swap_remove(best_idx));
    }

    selected.into_iter().map(|c| c.id).collect()
}

/// Load CLIP image embeddings for the given asset ids (missing ids simply omit).
pub fn load_embeddings(
    conn: &Connection,
    asset_ids: &[String],
) -> rusqlite::Result<HashMap<String, Vec<f32>>> {
    let mut out = HashMap::new();
    if asset_ids.is_empty() {
        return Ok(out);
    }
    // Batch in chunks — one query per chunk instead of one per asset.
    const CHUNK: usize = 200;
    for chunk in asset_ids.chunks(CHUNK) {
        let placeholders = std::iter::repeat("?")
        .take(chunk.len())
        .collect::<Vec<_>>()
        .join(",");
        let sql = format!(
            "SELECT asset_id, vector FROM asset_embeddings
             WHERE model_id = ?1 AND asset_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<rusqlite::types::Value> = Vec::with_capacity(1 + chunk.len());
        params.push(IMAGE_MODEL_ID.to_string().into());
        for id in chunk {
            params.push(id.clone().into());
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (aid, blob) = row?;
            if let Ok(v) = vector::decode(&blob) {
                out.insert(aid, v);
            }
        }
    }
    Ok(out)
}

/// Fetch caption text for assets in order; return first usable quote.
pub fn pick_quote(conn: &Connection, ordered_ids: &[String]) -> Option<String> {
    for id in ordered_ids.iter().take(16) {
        let caption: Option<String> = conn
            .query_row(
                "SELECT caption FROM asset_captions WHERE asset_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if let Some(raw) = caption {
            if let Some(q) = format_quote(&raw) {
                return Some(q);
            }
        }
    }
    None
}

pub fn format_quote(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.len() < 8 {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if lower == "caption"
        || lower == "what does the image describe?"
        || lower.starts_with("<caption")
    {
        return None;
    }
    let mut q = t.to_string();
    if q.len() > 120 {
        let mut end = 117;
        while end > 0 && !q.is_char_boundary(end) {
            end -= 1;
        }
        q.truncate(end);
        q.push('…');
    }
    Some(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_embeddings_favourite_wins() {
        let ids = diversify(
            vec![
                RankCandidate {
                    id: "a".into(),
                    favorite: false,
                    rating: 0,
                    has_thumb: true,
                    embedding: None,
                },
                RankCandidate {
                    id: "b".into(),
                    favorite: true,
                    rating: 0,
                    has_thumb: true,
                    embedding: None,
                },
                RankCandidate {
                    id: "c".into(),
                    favorite: false,
                    rating: 0,
                    has_thumb: true,
                    embedding: None,
                },
            ],
            3,
        );
        assert_eq!(ids[0], "b");
    }

    #[test]
    fn diversity_skips_near_duplicate_of_favourite() {
        let mut a_emb = vec![1.0f32, 0.0];
        crate::ml::vector::normalize(&mut a_emb);
        let mut b_emb = vec![0.99f32, 0.141];
        crate::ml::vector::normalize(&mut b_emb);
        let mut c_emb = vec![0.0f32, 1.0];
        crate::ml::vector::normalize(&mut c_emb);

        let ids = diversify(
            vec![
                RankCandidate {
                    id: "a".into(),
                    favorite: true,
                    rating: 0,
                    has_thumb: true,
                    embedding: Some(a_emb),
                },
                RankCandidate {
                    id: "b".into(),
                    favorite: true,
                    rating: 0,
                    has_thumb: true,
                    embedding: Some(b_emb),
                },
                RankCandidate {
                    id: "c".into(),
                    favorite: false,
                    rating: 0,
                    has_thumb: true,
                    embedding: Some(c_emb),
                },
            ],
            2,
        );
        assert_eq!(ids[0], "a");
        assert_eq!(
            ids[1], "c",
            "orthogonal shot should beat near-duplicate favourite"
        );
    }

    #[test]
    fn format_quote_rejects_junk_and_truncates() {
        assert!(format_quote("short").is_none());
        assert!(format_quote("CAPTION").is_none());
        let long = "a".repeat(200);
        let q = format_quote(&long).unwrap();
        assert!(q.ends_with('…'));
        assert!(q.chars().count() <= 118);
        assert_eq!(
            format_quote("A sunny afternoon by the harbour").as_deref(),
            Some("A sunny afternoon by the harbour")
        );
    }
}
