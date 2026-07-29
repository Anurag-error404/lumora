pub mod ffmpeg;

use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::error::AppResult;

pub const THUMB_MAX_EDGE: u32 = 320;

/// Oriented JPEG thumbs. The `.o` marker distinguishes them from older
/// unoriented `{hash}.jpg` files so startup repair can regenerate.
const THUMB_SUFFIX: &str = ".o.jpg";

pub fn thumbnail_path(thumbs_dir: &Path, hash: &str) -> PathBuf {
    thumbs_dir.join(format!("{hash}{THUMB_SUFFIX}"))
}

fn is_current_thumb(path: &str) -> bool {
    path.ends_with(THUMB_SUFFIX) && Path::new(path).is_file()
}

/// Open an image and bake EXIF/TIFF orientation into the pixel buffer.
///
/// Browsers apply orientation when showing the original; `image::open` does
/// not, which made library thumbnails appear rotated relative to the viewer.
pub fn open_oriented(path: &Path) -> AppResult<DynamicImage> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

pub fn generate_thumbnail(source: &Path, thumbs_dir: &Path, hash: &str) -> AppResult<PathBuf> {
    std::fs::create_dir_all(thumbs_dir)?;
    let dest = thumbnail_path(thumbs_dir, hash);
    if dest.exists() {
        return Ok(dest);
    }

    let img = open_oriented(source)?;
    write_thumbnail_jpeg(&img, &dest)?;
    Ok(dest)
}

/// Resize + JPEG-encode an already-decoded image. Shared by import so we never
/// decode the same photo twice (thumb + perceptual hash used to each open it).
pub fn write_thumbnail_jpeg(img: &DynamicImage, dest: &Path) -> AppResult<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let thumb = img.resize(THUMB_MAX_EDGE, THUMB_MAX_EDGE, FilterType::Triangle);
    thumb.save_with_format(dest, ImageFormat::Jpeg)?;
    Ok(())
}

/// Delete oldest thumbnail files until the cache is under `budget_mb` megabytes.
/// A budget of `0` means unlimited.
pub fn enforce_cache_budget(thumbs_dir: &Path, budget_mb: u32) {
    if budget_mb == 0 || !thumbs_dir.is_dir() {
        return;
    }
    let budget = (budget_mb as u64).saturating_mul(1024 * 1024);
    let mut entries: Vec<(std::path::PathBuf, u64, std::time::SystemTime)> =
        walkdir::WalkDir::new(thumbs_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let modified = meta.modified().ok()?;
                Some((e.into_path(), meta.len(), modified))
            })
            .collect();
    let mut total: u64 = entries.iter().map(|(_, len, _)| *len).sum();
    if total <= budget {
        return;
    }
    entries.sort_by_key(|(_, _, modified)| *modified);
    for (path, len, _) in entries {
        if total <= budget {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

/// Encode a thumbnail entirely in memory. Used by the privacy vault, where
/// writing a plaintext preview to the thumbnail cache would defeat the point.
pub fn thumbnail_bytes(source: &Path) -> AppResult<Vec<u8>> {
    let img = open_oriented(source)?;
    let thumb = img.resize(THUMB_MAX_EDGE, THUMB_MAX_EDGE, FilterType::Triangle);
    let mut buffer = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut buffer, ImageFormat::Jpeg)?;
    Ok(buffer.into_inner())
}

/// Recreate thumbnail files that are missing or still on the pre-orientation naming.
/// Videos use ffmpeg frame extraction when available.
pub fn repair_missing_thumbnails(conn: &rusqlite::Connection, thumbs_dir: &Path) -> AppResult<u32> {
    let mut stmt = conn.prepare(
        "SELECT id, path, hash, thumbnail_path, media_type FROM assets
         WHERE deleted_at IS NULL
           AND media_type IN ('image', 'video')
           AND hash IS NOT NULL
           AND hash != ''",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;

    let mut repaired = 0u32;
    for row in rows.flatten() {
        let (id, path, hash, thumb_path, media_type) = row;
        let needs = match &thumb_path {
            Some(p) => !is_current_thumb(p),
            None => true,
        };
        if !needs {
            continue;
        }
        let source = Path::new(&path);
        if !source.is_file() {
            continue;
        }
        // Drop stale unoriented sibling if present.
        if let Some(old) = &thumb_path {
            if !old.ends_with(THUMB_SUFFIX) {
                let _ = std::fs::remove_file(old);
            }
        }
        let legacy = thumbs_dir.join(format!("{hash}.jpg"));
        if legacy.exists() {
            let _ = std::fs::remove_file(&legacy);
        }
        // Force regenerate even if a leftover current file exists with wrong pixels.
        let dest = thumbnail_path(thumbs_dir, &hash);
        let _ = std::fs::remove_file(&dest);
        let generated = if media_type == "video" {
            ffmpeg::extract_frame_thumbnail(source, thumbs_dir, &hash)
        } else {
            generate_thumbnail(source, thumbs_dir, &hash)
        };
        if let Ok(dest) = generated {
            let dest_str = dest.to_string_lossy().to_string();
            conn.execute(
                "UPDATE assets SET thumbnail_path = ?1 WHERE id = ?2",
                rusqlite::params![dest_str, id],
            )?;
            repaired += 1;
        }
    }
    Ok(repaired)
}

/// aHash from an already-decoded image (avoids a second full decode on import).
pub fn perceptual_hash_from_image(img: &DynamicImage) -> String {
    let small = img.resize_exact(8, 8, FilterType::Nearest).to_luma8();
    let pixels: Vec<u8> = small.pixels().map(|p| p.0[0]).collect();
    let avg = (pixels.iter().map(|&p| p as u32).sum::<u32>() / pixels.len() as u32) as u8;
    let mut bits: u64 = 0;
    for (i, &p) in pixels.iter().enumerate() {
        if p >= avg {
            bits |= 1u64 << i;
        }
    }
    format!("{bits:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, Rgb, RgbImage};
    use tempfile::tempdir;

    #[test]
    fn resize_writes_oriented_jpeg_under_thumbs() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src.png");
        let img = RgbImage::from_pixel(800, 600, Rgb([10, 20, 30]));
        img.save(&src).unwrap();

        let thumbs = dir.path().join("thumbs");
        let out = generate_thumbnail(&src, &thumbs, "abc123").unwrap();
        assert!(out.exists());
        assert!(out.to_string_lossy().ends_with(".o.jpg"));

        let (w, h) = image::image_dimensions(&out).unwrap();
        assert!(w <= THUMB_MAX_EDGE && h <= THUMB_MAX_EDGE);
    }

    #[test]
    fn open_oriented_matches_plain_open_without_exif() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("plain.png");
        RgbImage::from_pixel(40, 20, Rgb([1, 2, 3]))
            .save(&src)
            .unwrap();
        let a = open_oriented(&src).unwrap();
        let b = image::open(&src).unwrap();
        assert_eq!(a.dimensions(), b.dimensions());
    }

    #[test]
    fn repair_upgrades_legacy_unoriented_thumb_paths() {
        use crate::db;
        use rusqlite::params;

        let dir = tempdir().unwrap();
        let src = dir.path().join("src.png");
        RgbImage::from_pixel(64, 64, Rgb([1, 2, 3]))
            .save(&src)
            .unwrap();
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        let hash = "deadbeef";
        let legacy = thumbs.join(format!("{hash}.jpg"));
        // Fake a legacy thumb file that already exists.
        RgbImage::from_pixel(8, 8, Rgb([9, 9, 9]))
            .save(&legacy)
            .unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, thumbnail_path)
             VALUES ('1', ?1, ?2, 'image', 't', 't', ?3)",
            params![
                src.to_string_lossy().to_string(),
                hash,
                legacy.to_string_lossy().to_string()
            ],
        )
        .unwrap();

        let repaired = repair_missing_thumbnails(&conn, &thumbs).unwrap();
        assert_eq!(repaired, 1);
        let current = thumbnail_path(&thumbs, hash);
        assert!(current.exists());
        assert!(!legacy.exists());
        let stored: String = conn
            .query_row(
                "SELECT thumbnail_path FROM assets WHERE id = '1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stored.ends_with(".o.jpg"));
    }

    #[test]
    fn repair_missing_thumbnails_regenerates_file() {
        use crate::db;
        use rusqlite::params;

        let dir = tempdir().unwrap();
        let src = dir.path().join("src.png");
        RgbImage::from_pixel(64, 64, Rgb([1, 2, 3]))
            .save(&src)
            .unwrap();
        let thumbs = dir.path().join("thumbs");
        let hash = "deadbeef";
        let missing = thumbnail_path(&thumbs, hash);

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        conn.execute(
            "INSERT INTO assets (id, path, hash, media_type, created_at, indexed_at, thumbnail_path)
             VALUES ('1', ?1, ?2, 'image', 't', 't', ?3)",
            params![
                src.to_string_lossy().to_string(),
                hash,
                missing.to_string_lossy().to_string()
            ],
        )
        .unwrap();

        assert!(!missing.exists());
        let repaired = repair_missing_thumbnails(&conn, &thumbs).unwrap();
        assert_eq!(repaired, 1);
        assert!(missing.exists());
    }
}
