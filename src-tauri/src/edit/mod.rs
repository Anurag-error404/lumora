//! Image edits: non-destructive ops history plus optional bake to disk.
//!
//! **Save edits** appends JSON ops to `asset_edits` without touching pixels.
//! **Bake** (`apply_edit` replace/copy) rewrites pixels, then clears that
//! asset's revision rows. After bake we re-upsert so hash/thumbs/metadata stay
//! correct, drop CLIP/OCR/faces/tags derived data, and kick workers to rebuild.

use std::path::{Path, PathBuf};

use chrono::Utc;
use image::imageops;
use image::{DynamicImage, ImageFormat, RgbaImage};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::indexer;
use crate::ml::catalog::ModelKind;
use crate::models::AssetSummary;
use crate::search;
use crate::semantic;

/// Normalized crop rectangle in the post-rotate / post-flip image space (0…1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOps {
    /// Clockwise degrees: 0, 90, 180, or 270.
    pub rotate_degrees: i32,
    /// Mirror left↔right after rotation.
    #[serde(default)]
    pub flip_horizontal: bool,
    /// Mirror top↔bottom after rotation.
    #[serde(default)]
    pub flip_vertical: bool,
    /// Optional crop after rotation/flip. Omitted / full frame = no crop.
    pub crop: Option<CropRect>,
    /// Exposure compensation in stops (−2…+2). `0` leaves brightness alone.
    pub exposure: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveMode {
    Replace,
    Copy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub asset: AssetSummary,
    pub mode: SaveMode,
    /// True when a previous CLIP embedding was cleared and will be rebuilt.
    pub embedding_queued: bool,
}

/// One append-only revision of non-destructive edit ops.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedEditOps {
    pub asset_id: String,
    pub revision_id: String,
    pub ops: EditOps,
    pub created_at: String,
}

/// Thin history strip entry (newest first).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRevisionSummary {
    pub id: String,
    pub created_at: String,
}

fn ensure_image_asset(conn: &Connection, asset_id: &str) -> AppResult<()> {
    let media_type: String = conn
        .query_row(
            "SELECT media_type FROM assets WHERE id = ?1 AND deleted_at IS NULL",
            params![asset_id],
            |r| r.get(0),
        )
        .map_err(|_| AppError::msg("asset not found"))?;
    if media_type != "image" {
        return Err(AppError::msg("only images can be edited"));
    }
    Ok(())
}

fn validate_ops(ops: &EditOps) -> AppResult<()> {
    let _ = normalize_rotate(ops.rotate_degrees)?;
    if ops.exposure.abs() > 2.0 {
        return Err(AppError::msg("exposure must be between -2 and +2 stops"));
    }
    if let Some(crop) = &ops.crop {
        if crop.width <= 0.0 || crop.height <= 0.0 {
            return Err(AppError::msg("crop size must be positive"));
        }
    }
    Ok(())
}

/// Append a revision of edit ops without touching the original file.
pub fn save_edit_ops(
    conn: &Connection,
    asset_id: &str,
    ops: &EditOps,
) -> AppResult<SavedEditOps> {
    ensure_image_asset(conn, asset_id)?;
    validate_ops(ops)?;
    let ops_json =
        serde_json::to_string(ops).map_err(|e| AppError::msg(format!("ops json: {e}")))?;
    let revision_id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO asset_edits (id, asset_id, ops_json, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![revision_id, asset_id, ops_json, created_at],
    )?;
    Ok(SavedEditOps {
        asset_id: asset_id.to_string(),
        revision_id,
        ops: ops.clone(),
        created_at,
    })
}

/// Latest ops for an asset, or `None` when no revisions exist.
pub fn get_edit_ops(conn: &Connection, asset_id: &str) -> AppResult<Option<SavedEditOps>> {
    ensure_image_asset(conn, asset_id)?;
    let row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT id, ops_json, created_at FROM asset_edits
             WHERE asset_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            params![asset_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((revision_id, ops_json, created_at)) => {
            let ops: EditOps = serde_json::from_str(&ops_json)
                .map_err(|e| AppError::msg(format!("ops json: {e}")))?;
            Ok(Some(SavedEditOps {
                asset_id: asset_id.to_string(),
                revision_id,
                ops,
                created_at,
            }))
        }
    }
}

/// Newest-first revision list for the history strip.
pub fn list_edit_revisions(
    conn: &Connection,
    asset_id: &str,
) -> AppResult<Vec<EditRevisionSummary>> {
    ensure_image_asset(conn, asset_id)?;
    let mut stmt = conn.prepare(
        "SELECT id, created_at FROM asset_edits
         WHERE asset_id = ?1
         ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt.query_map(params![asset_id], |r| {
        Ok(EditRevisionSummary {
            id: r.get(0)?,
            created_at: r.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Append a copy of an older revision’s ops as the new latest.
pub fn revert_edit_revision(
    conn: &Connection,
    asset_id: &str,
    revision_id: &str,
) -> AppResult<SavedEditOps> {
    ensure_image_asset(conn, asset_id)?;
    let ops_json: String = conn
        .query_row(
            "SELECT ops_json FROM asset_edits WHERE id = ?1 AND asset_id = ?2",
            params![revision_id, asset_id],
            |r| r.get(0),
        )
        .map_err(|_| AppError::msg("edit revision not found"))?;
    let ops: EditOps = serde_json::from_str(&ops_json)
        .map_err(|e| AppError::msg(format!("ops json: {e}")))?;
    save_edit_ops(conn, asset_id, &ops)
}

/// Delete all revisions for an asset (reset / after bake).
pub fn clear_edit_ops(conn: &Connection, asset_id: &str) -> AppResult<()> {
    // Allow clear even if asset was deleted mid-flow; still require it exists as image when present.
    let exists: Option<String> = conn
        .query_row(
            "SELECT media_type FROM assets WHERE id = ?1",
            params![asset_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(media_type) = exists {
        if media_type != "image" {
            return Err(AppError::msg("only images can be edited"));
        }
    }
    conn.execute(
        "DELETE FROM asset_edits WHERE asset_id = ?1",
        params![asset_id],
    )?;
    Ok(())
}

/// Bake edits to disk (replace or sibling copy). Videos and missing files are rejected.
/// Clears non-destructive revision rows for the source asset after a successful write.
pub fn apply_edit(
    conn: &Connection,
    thumbs_dir: &Path,
    asset_id: &str,
    ops: &EditOps,
    mode: SaveMode,
) -> AppResult<EditResult> {
    let (path_str, media_type): (String, String) = conn.query_row(
        "SELECT path, media_type FROM assets WHERE id = ?1 AND deleted_at IS NULL",
        params![asset_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if media_type != "image" {
        return Err(AppError::msg("only images can be edited"));
    }
    let source = PathBuf::from(&path_str);
    if !source.is_file() {
        return Err(AppError::msg("image file is missing on disk"));
    }

    let img = crate::thumbnails::open_oriented(&source)
        .map_err(|e| AppError::msg(format!("open image: {e}")))?;
    let edited = apply_ops(img, ops)?;

    let dest = match mode {
        SaveMode::Replace => {
            let tmp = source.with_extension("lumora-edit-tmp");
            write_image(&edited, &tmp, preferred_format(&source))?;
            std::fs::rename(&tmp, &source).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                AppError::msg(format!("replace original: {e}"))
            })?;
            source.clone()
        }
        SaveMode::Copy => {
            let dest = unique_copy_path(&source);
            write_image(&edited, &dest, preferred_format(&source))?;
            dest
        }
    };

    // Force thumb regeneration even if content somehow matched.
    let outcome = indexer::upsert_asset(conn, &dest, thumbs_dir, true)?;
    let id = match mode {
        SaveMode::Replace => asset_id.to_string(),
        SaveMode::Copy => {
            // upsert inserts by path; resolve the new id.
            let path_str = dest.to_string_lossy().to_string();
            conn.query_row(
                "SELECT id FROM assets WHERE path = ?1",
                params![path_str],
                |r| r.get(0),
            )?
        }
    };
    let _ = outcome;

    // Pixels now embody the ops — drop sidecar revisions on the source asset.
    clear_edit_ops(conn, asset_id)?;

    let embedding_queued = invalidate_embedding(conn, &id)?;
    let _ = crate::ocr::invalidate_asset(conn, &id);
    // faces_dir is known by the caller via AppPaths; edit module gets it from thumbs sibling.
    // Invalidate faces using the standard app_data/faces layout relative to thumbs.
    if let Some(app_data) = thumbs_dir.parent() {
        let faces_dir = app_data.join("faces");
        let _ = crate::faces::invalidate_asset(conn, &faces_dir, &id);
    }
    let _ = crate::tags::invalidate_asset(conn, &id);
    let asset = load_asset(conn, &id)?;
    Ok(EditResult {
        asset,
        mode,
        embedding_queued,
    })
}

pub fn apply_ops(img: DynamicImage, ops: &EditOps) -> AppResult<DynamicImage> {
    let mut out = match normalize_rotate(ops.rotate_degrees)? {
        0 => img,
        90 => DynamicImage::ImageRgba8(imageops::rotate90(&img)),
        180 => DynamicImage::ImageRgba8(imageops::rotate180(&img)),
        270 => DynamicImage::ImageRgba8(imageops::rotate270(&img)),
        _ => unreachable!(),
    };

    if ops.flip_horizontal {
        out = DynamicImage::ImageRgba8(imageops::flip_horizontal(&out));
    }
    if ops.flip_vertical {
        out = DynamicImage::ImageRgba8(imageops::flip_vertical(&out));
    }

    if let Some(crop) = &ops.crop {
        out = crop_normalized(out, crop)?;
    }

    if ops.exposure.abs() > 0.001 {
        out = apply_exposure(out, ops.exposure);
    }

    Ok(out)
}

fn normalize_rotate(degrees: i32) -> AppResult<i32> {
    let d = ((degrees % 360) + 360) % 360;
    match d {
        0 | 90 | 180 | 270 => Ok(d),
        _ => Err(AppError::msg("rotate must be 0, 90, 180, or 270 degrees")),
    }
}

fn crop_normalized(img: DynamicImage, crop: &CropRect) -> AppResult<DynamicImage> {
    if crop.width <= 0.0 || crop.height <= 0.0 {
        return Err(AppError::msg("crop size must be positive"));
    }
    let (w, h) = (img.width(), img.height());
    let x = (crop.x.clamp(0.0, 1.0) * w as f32).round() as u32;
    let y = (crop.y.clamp(0.0, 1.0) * h as f32).round() as u32;
    let mut cw = (crop.width.clamp(0.0, 1.0) * w as f32).round() as u32;
    let mut ch = (crop.height.clamp(0.0, 1.0) * h as f32).round() as u32;
    if x >= w || y >= h {
        return Err(AppError::msg("crop is outside the image"));
    }
    cw = cw.min(w - x).max(1);
    ch = ch.min(h - y).max(1);
    Ok(DynamicImage::ImageRgba8(
        imageops::crop_imm(&img, x, y, cw, ch).to_image(),
    ))
}

fn apply_exposure(img: DynamicImage, stops: f32) -> DynamicImage {
    let stops = stops.clamp(-2.0, 2.0);
    let factor = 2f32.powf(stops);
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = RgbaImage::new(w, h);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let r = ((pixel[0] as f32) * factor).round().clamp(0.0, 255.0) as u8;
        let g = ((pixel[1] as f32) * factor).round().clamp(0.0, 255.0) as u8;
        let b = ((pixel[2] as f32) * factor).round().clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, image::Rgba([r, g, b, pixel[3]]));
    }
    DynamicImage::ImageRgba8(out)
}

fn preferred_format(path: &Path) -> ImageFormat {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => ImageFormat::Png,
        Some("gif") => ImageFormat::Gif,
        Some("webp") => ImageFormat::WebP,
        Some("bmp") => ImageFormat::Bmp,
        _ => ImageFormat::Jpeg,
    }
}

fn write_image(img: &DynamicImage, path: &Path, format: ImageFormat) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match format {
        ImageFormat::Jpeg => {
            let rgb = img.to_rgb8();
            let mut file = std::fs::File::create(path)?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 92);
            encoder
                .encode(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| AppError::msg(format!("jpeg encode: {e}")))?;
        }
        other => {
            img.save_with_format(path, other)
                .map_err(|e| AppError::msg(format!("save image: {e}")))?;
        }
    }
    Ok(())
}

fn unique_copy_path(source: &Path) -> PathBuf {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("edited");
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let candidate = parent.join(format!("{stem}_edited.{ext}"));
    if !candidate.exists() {
        return candidate;
    }
    for i in 2..10_000 {
        let candidate = parent.join(format!("{stem}_edited_{i}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(
        "{stem}_edited_{}.{}",
        uuid::Uuid::new_v4(),
        ext
    ))
}

/// Drop the CLIP vector so `pending_assets` picks the asset up again.
pub fn invalidate_embedding(conn: &Connection, asset_id: &str) -> AppResult<bool> {
    let removed = conn.execute(
        "DELETE FROM asset_embeddings WHERE asset_id = ?1 AND model_id = ?2",
        params![asset_id, semantic::IMAGE_MODEL_ID],
    )?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE asset_id = ?1 AND kind = ?2",
        params![asset_id, ModelKind::ClipImage.as_str()],
    )?;
    Ok(removed > 0)
}

fn load_asset(conn: &Connection, id: &str) -> AppResult<AssetSummary> {
    conn.query_row(
        "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                created_at, captured_at, indexed_at, favorite, rating, color_label,
                thumbnail_path, camera, lens, deleted_at
         FROM assets WHERE id = ?1",
        params![id],
        search::map_asset,
    )
    .map_err(|_| AppError::msg("edited asset not found after save"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use image::{Rgb, RgbImage};
    use tempfile::tempdir;

    fn seed_image(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let path = dir.join(name);
        let mut img = RgbImage::new(w, h);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([(x % 256) as u8, (y % 256) as u8, 80]);
        }
        DynamicImage::ImageRgb8(img)
            .save_with_format(&path, ImageFormat::Jpeg)
            .unwrap();
        path
    }

    fn open_db_with_asset(dir: &Path, path: &Path) -> (Connection, String) {
        let conn = db::open_and_migrate(&dir.join("library.db")).unwrap();
        let thumbs = dir.join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        indexer::upsert_asset(&conn, path, &thumbs, true).unwrap();
        let id: String = conn
            .query_row(
                "SELECT id FROM assets WHERE path = ?1",
                params![path.display().to_string()],
                |r| r.get(0),
            )
            .unwrap();
        (conn, id)
    }

    #[test]
    fn rotate_90_swaps_dimensions() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(40, 20, Rgb([10, 20, 30])));
        let out = apply_ops(
            img,
            &EditOps {
                rotate_degrees: 90,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 0.0,
            },
        )
        .unwrap();
        assert_eq!((out.width(), out.height()), (20, 40));
    }

    #[test]
    fn crop_reduces_size() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(100, 100, Rgb([1, 2, 3])));
        let out = apply_ops(
            img,
            &EditOps {
                rotate_degrees: 0,
                flip_horizontal: false,
                flip_vertical: false,
                crop: Some(CropRect {
                    x: 0.25,
                    y: 0.25,
                    width: 0.5,
                    height: 0.5,
                }),
                exposure: 0.0,
            },
        )
        .unwrap();
        assert_eq!((out.width(), out.height()), (50, 50));
    }

    #[test]
    fn flip_horizontal_mirrors_pixels() {
        let mut img = RgbImage::new(2, 1);
        img.put_pixel(0, 0, Rgb([10, 0, 0]));
        img.put_pixel(1, 0, Rgb([200, 0, 0]));
        let out = apply_ops(
            DynamicImage::ImageRgb8(img),
            &EditOps {
                rotate_degrees: 0,
                flip_horizontal: true,
                flip_vertical: false,
                crop: None,
                exposure: 0.0,
            },
        )
        .unwrap()
        .to_rgb8();
        assert_eq!(out.get_pixel(0, 0).0[0], 200);
        assert_eq!(out.get_pixel(1, 0).0[0], 10);
    }

    #[test]
    fn exposure_brightens_dark_pixel() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, Rgb([40, 40, 40])));
        let out = apply_ops(
            img,
            &EditOps {
                rotate_degrees: 0,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 1.0,
            },
        )
        .unwrap();
        let p = out.to_rgb8().get_pixel(0, 0).0;
        assert!(p[0] > 70, "got {:?}", p);
    }

    #[test]
    fn replace_updates_hash_and_invalidates_embedding() {
        let dir = tempdir().unwrap();
        let path = seed_image(dir.path(), "shot.jpg", 32, 24);
        let (conn, id) = open_db_with_asset(dir.path(), &path);
        let thumbs = dir.path().join("thumbs");

        // Fake an existing embedding so invalidate reports true.
        conn.execute(
            "INSERT INTO asset_embeddings (asset_id, model_id, dim, vector, created_at)
             VALUES (?1, ?2, 2, ?3, '2026-01-01T00:00:00Z')",
            params![id, semantic::IMAGE_MODEL_ID, vec![0u8; 8]],
        )
        .unwrap();

        let before: String = conn
            .query_row("SELECT hash FROM assets WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .unwrap();

        let result = apply_edit(
            &conn,
            &thumbs,
            &id,
            &EditOps {
                rotate_degrees: 90,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 0.5,
            },
            SaveMode::Replace,
        )
        .unwrap();

        assert_eq!(result.asset.id, id);
        assert!(result.embedding_queued);
        let after: String = conn
            .query_row("SELECT hash FROM assets WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_ne!(before, after);
        let emb: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM asset_embeddings WHERE asset_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(emb, 0);
        let (w, h) = image::image_dimensions(&path).unwrap();
        assert_eq!((w, h), (24, 32));
    }

    #[test]
    fn copy_creates_sibling_and_new_asset() {
        let dir = tempdir().unwrap();
        let path = seed_image(dir.path(), "original.jpg", 30, 30);
        let (conn, id) = open_db_with_asset(dir.path(), &path);
        let thumbs = dir.path().join("thumbs");

        let result = apply_edit(
            &conn,
            &thumbs,
            &id,
            &EditOps {
                rotate_degrees: 180,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 0.0,
            },
            SaveMode::Copy,
        )
        .unwrap();

        assert_ne!(result.asset.id, id);
        assert!(result.asset.path.contains("_edited"));
        assert!(Path::new(&result.asset.path).is_file());
        assert!(path.is_file(), "original must remain");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn save_get_edit_ops_round_trip() {
        let dir = tempdir().unwrap();
        let path = seed_image(dir.path(), "ops.jpg", 20, 20);
        let (conn, id) = open_db_with_asset(dir.path(), &path);

        let ops = EditOps {
            rotate_degrees: 90,
            flip_horizontal: true,
            flip_vertical: false,
            crop: Some(CropRect {
                x: 0.1,
                y: 0.2,
                width: 0.5,
                height: 0.6,
            }),
            exposure: 0.5,
        };
        let saved = save_edit_ops(&conn, &id, &ops).unwrap();
        assert_eq!(saved.asset_id, id);
        assert!(!saved.revision_id.is_empty());

        let latest = get_edit_ops(&conn, &id).unwrap().expect("ops present");
        assert_eq!(latest.revision_id, saved.revision_id);
        assert_eq!(latest.ops.rotate_degrees, 90);
        assert!(latest.ops.flip_horizontal);
        assert_eq!(latest.ops.exposure, 0.5);
        let crop = latest.ops.crop.as_ref().expect("crop");
        assert!((crop.x - 0.1).abs() < 0.001);
    }

    #[test]
    fn append_list_and_revert_edit_ops() {
        let dir = tempdir().unwrap();
        let path = seed_image(dir.path(), "hist.jpg", 16, 16);
        let (conn, id) = open_db_with_asset(dir.path(), &path);

        let first = save_edit_ops(
            &conn,
            &id,
            &EditOps {
                rotate_degrees: 90,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 0.0,
            },
        )
        .unwrap();
        // Ensure distinct created_at ordering if clock resolution is coarse.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = save_edit_ops(
            &conn,
            &id,
            &EditOps {
                rotate_degrees: 180,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 1.0,
            },
        )
        .unwrap();

        let list = list_edit_revisions(&conn, &id).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, second.revision_id);
        assert_eq!(list[1].id, first.revision_id);

        let reverted = revert_edit_revision(&conn, &id, &first.revision_id).unwrap();
        assert_eq!(reverted.ops.rotate_degrees, 90);
        assert_eq!(reverted.ops.exposure, 0.0);
        let latest = get_edit_ops(&conn, &id).unwrap().unwrap();
        assert_eq!(latest.revision_id, reverted.revision_id);
        assert_eq!(latest.ops.rotate_degrees, 90);
        assert_eq!(list_edit_revisions(&conn, &id).unwrap().len(), 3);
    }

    #[test]
    fn bake_replace_clears_edit_ops() {
        let dir = tempdir().unwrap();
        let path = seed_image(dir.path(), "bake.jpg", 32, 24);
        let (conn, id) = open_db_with_asset(dir.path(), &path);
        let thumbs = dir.path().join("thumbs");

        save_edit_ops(
            &conn,
            &id,
            &EditOps {
                rotate_degrees: 90,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 0.0,
            },
        )
        .unwrap();
        assert_eq!(list_edit_revisions(&conn, &id).unwrap().len(), 1);

        let before: String = conn
            .query_row("SELECT hash FROM assets WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .unwrap();

        apply_edit(
            &conn,
            &thumbs,
            &id,
            &EditOps {
                rotate_degrees: 90,
                flip_horizontal: false,
                flip_vertical: false,
                crop: None,
                exposure: 0.0,
            },
            SaveMode::Replace,
        )
        .unwrap();

        let after: String = conn
            .query_row("SELECT hash FROM assets WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_ne!(before, after);
        assert!(get_edit_ops(&conn, &id).unwrap().is_none());
        assert!(list_edit_revisions(&conn, &id).unwrap().is_empty());
    }
}
