//! Blur detection via variance of Laplacian (no ML download).
//!
//! Lower scores mean softer / more out-of-focus images. Scores are computed on
//! a normalized grayscale preview so thresholds stay comparable across sizes.

use std::path::Path;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, GrayImage};
use rusqlite::{params, Connection};

use crate::error::AppResult;
use crate::models::BlurryAsset;
use crate::search;
use crate::thumbnails;

/// Max edge length used when scoring (keeps scores comparable).
const SCORE_MAX_EDGE: u32 = 256;

/// Images at or below this Laplacian variance are treated as blurry.
pub const BLURRY_THRESHOLD: f64 = 80.0;

pub fn blur_score_from_image(img: &DynamicImage) -> f64 {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let gray = if w > SCORE_MAX_EDGE || h > SCORE_MAX_EDGE {
        img.resize(SCORE_MAX_EDGE, SCORE_MAX_EDGE, FilterType::Triangle)
            .to_luma8()
    } else {
        img.to_luma8()
    };
    laplacian_variance(&gray)
}

fn laplacian_variance(gray: &GrayImage) -> f64 {
    let (w, h) = gray.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }

    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut n = 0.0f64;

    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = gray.get_pixel(x, y).0[0] as f64;
            let up = gray.get_pixel(x, y - 1).0[0] as f64;
            let down = gray.get_pixel(x, y + 1).0[0] as f64;
            let left = gray.get_pixel(x - 1, y).0[0] as f64;
            let right = gray.get_pixel(x + 1, y).0[0] as f64;
            let lap = up + down + left + right - 4.0 * c;
            sum += lap;
            sum_sq += lap * lap;
            n += 1.0;
        }
    }

    if n < 1.0 {
        return 0.0;
    }
    let mean = sum / n;
    (sum_sq / n) - (mean * mean)
}

/// Score from a file path (full image, oriented).
pub fn blur_score_path(path: &Path) -> AppResult<f64> {
    let img = thumbnails::open_oriented(path)?;
    Ok(blur_score_from_image(&img))
}

/// Prefer an existing thumbnail for speed; fall back to the original.
pub fn blur_score_for_asset(path: &Path, thumbnail_path: Option<&str>) -> AppResult<f64> {
    if let Some(thumb) = thumbnail_path {
        let thumb_path = Path::new(thumb);
        if thumb_path.is_file() {
            if let Ok(img) = image::open(thumb_path) {
                return Ok(blur_score_from_image(&img));
            }
        }
    }
    blur_score_path(path)
}

/// Fill `blur_score` for images that still lack one (up to `limit` rows).
pub fn backfill_missing(conn: &Connection, limit: u32) -> AppResult<usize> {
    let mut stmt = conn.prepare(
        "SELECT id, path, thumbnail_path FROM assets
         WHERE deleted_at IS NULL
           AND media_type = 'image'
           AND blur_score IS NULL
         LIMIT ?1",
    )?;
    let rows: Vec<(String, String, Option<String>)> = stmt
        .query_map(params![limit], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut updated = 0usize;
    for (id, path, thumb) in rows {
        match blur_score_for_asset(Path::new(&path), thumb.as_deref()) {
            Ok(score) => {
                conn.execute(
                    "UPDATE assets SET blur_score = ?1 WHERE id = ?2",
                    params![score, id],
                )?;
                updated += 1;
            }
            Err(e) => {
                tracing::debug!(asset = %id, error = %e, "blur score skipped");
            }
        }
    }
    Ok(updated)
}

/// Live images whose blur score is at or below the blurry threshold.
pub fn list_blurry(conn: &Connection, limit: u32, offset: u32) -> AppResult<Vec<BlurryAsset>> {
    // Opportunistically score a batch of unscored images before listing.
    let _ = backfill_missing(conn, 200);

    let mut stmt = conn.prepare(
        "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                created_at, captured_at, indexed_at, favorite, rating, color_label,
                thumbnail_path, camera, lens, deleted_at, blur_score
         FROM assets
         WHERE deleted_at IS NULL
           AND media_type = 'image'
           AND blur_score IS NOT NULL
           AND blur_score <= ?1
         ORDER BY blur_score ASC, created_at DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt.query_map(params![BLURRY_THRESHOLD, limit, offset], |row| {
        let asset = search::map_asset(row)?;
        let blur_score: f64 = row.get(18)?;
        Ok(BlurryAsset { asset, blur_score })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn sharp_checkerboard_scores_higher_than_flat_color() {
        let mut sharp = RgbImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = if (x / 4 + y / 4) % 2 == 0 { 255 } else { 0 };
                sharp.put_pixel(x, y, Rgb([v, v, v]));
            }
        }
        let flat = RgbImage::from_pixel(64, 64, Rgb([128, 128, 128]));
        let sharp_score = blur_score_from_image(&DynamicImage::ImageRgb8(sharp));
        let flat_score = blur_score_from_image(&DynamicImage::ImageRgb8(flat));
        assert!(
            sharp_score > flat_score + 50.0,
            "sharp={sharp_score} flat={flat_score}"
        );
        assert!(flat_score < BLURRY_THRESHOLD);
        assert!(sharp_score > BLURRY_THRESHOLD);
    }
}
