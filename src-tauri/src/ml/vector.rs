//! Embedding storage and similarity.
//!
//! Vectors are stored L2-normalised as little-endian `f32`. Because every
//! stored vector is unit length, cosine similarity is just a dot product —
//! no per-query normalisation, no square roots in the hot loop.

use crate::error::{AppError, AppResult};

/// Scale a vector to unit length in place.
///
/// A zero vector has no direction, so it is left untouched rather than turned
/// into NaNs; callers treat it as "no signal".
pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

pub fn encode(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

pub fn decode(bytes: &[u8]) -> AppResult<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return Err(AppError::msg(format!(
            "embedding blob is {} bytes, not a whole number of f32s",
            bytes.len()
        )));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// Cosine similarity for unit-length vectors. Mismatched widths score 0 rather
/// than panicking, so one bad row can never take down a search.
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Keep the `limit` highest-scoring items.
///
/// Uses partial selection rather than a full sort: at 1M assets the difference
/// between ranking everything and selecting the top 100 is substantial.
pub fn top_k<T>(mut scored: Vec<(T, f32)>, limit: usize) -> Vec<(T, f32)> {
    if limit == 0 {
        return Vec::new();
    }
    if scored.len() > limit {
        scored.select_nth_unstable_by(limit - 1, |a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
    }
    scored.sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn normalize_gives_unit_length() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v);
        assert!(approx(v[0], 0.6) && approx(v[1], 0.8));
        let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(approx(len, 1.0));
    }

    #[test]
    fn normalize_leaves_a_zero_vector_alone_instead_of_producing_nan() {
        let mut v = vec![0.0, 0.0, 0.0];
        normalize(&mut v);
        assert!(v.iter().all(|x| *x == 0.0), "got {v:?}");
        assert!(v.iter().all(|x| !x.is_nan()));
    }

    #[test]
    fn encode_decode_round_trips_exactly() {
        let v = vec![0.5, -0.25, 1.0, 0.0, -1.0];
        let decoded = decode(&encode(&v)).unwrap();
        assert_eq!(decoded, v, "f32 round trip must be lossless");
    }

    #[test]
    fn decode_rejects_a_truncated_blob() {
        let err = decode(&[1, 2, 3]).unwrap_err();
        assert!(err.to_string().contains("f32"), "{err}");
    }

    #[test]
    fn identical_directions_score_one_and_opposites_score_minus_one() {
        let mut a = vec![1.0, 2.0, 3.0];
        let mut b = vec![2.0, 4.0, 6.0];
        let mut c = vec![-1.0, -2.0, -3.0];
        normalize(&mut a);
        normalize(&mut b);
        normalize(&mut c);

        assert!(approx(similarity(&a, &b), 1.0), "same direction");
        assert!(approx(similarity(&a, &c), -1.0), "opposite direction");
    }

    #[test]
    fn orthogonal_vectors_score_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(approx(similarity(&a, &b), 0.0));
    }

    #[test]
    fn mismatched_widths_score_zero_rather_than_panicking() {
        assert_eq!(similarity(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn top_k_returns_the_best_scores_in_descending_order() {
        let scored = vec![("a", 0.1), ("b", 0.9), ("c", 0.5), ("d", 0.7)];
        let top = top_k(scored, 2);
        assert_eq!(
            top.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            ["b", "d"]
        );
    }

    #[test]
    fn top_k_handles_limits_larger_than_the_input_and_zero() {
        let scored = vec![("a", 0.1), ("b", 0.9)];
        assert_eq!(top_k(scored.clone(), 10).len(), 2);
        assert!(top_k(scored, 0).is_empty());
    }

    #[test]
    fn top_k_is_stable_against_nan_scores() {
        let scored = vec![("a", f32::NAN), ("b", 0.9), ("c", 0.5)];
        let top = top_k(scored, 2);
        assert_eq!(top.len(), 2, "NaN must not drop or duplicate rows");
    }
}
