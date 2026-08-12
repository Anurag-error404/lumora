pub mod ffmpeg;
pub mod heic;
pub mod raw;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use parking_lot::Mutex;

use crate::error::{AppError, AppResult};
use crate::models::ThumbnailFailure;

pub const THUMB_MAX_EDGE: u32 = 320;

/// Oriented JPEG thumbs. The `.o` marker distinguishes them from older
/// unoriented `{hash}.jpg` files so startup repair can regenerate.
const THUMB_SUFFIX: &str = ".o.jpg";

const THUMB_ATTEMPTS: u32 = 3;
const THUMB_BACKOFF_MS: [u64; 2] = [50, 150];
const MAX_RECENT_FAILURES: usize = 100;

static RECENT_FAILURES: Mutex<VecDeque<ThumbnailFailure>> = Mutex::new(VecDeque::new());

pub fn recent_failures() -> Vec<ThumbnailFailure> {
    RECENT_FAILURES.lock().iter().cloned().collect()
}

/// Record a thumbnail failure when generation never reached the retry helper
/// (e.g. image decode failed before write).
pub fn record_failure_for_path(
    asset_id: Option<&str>,
    source: &Path,
    media_type: &str,
    error: &str,
) {
    record_failure(asset_id, source, media_type, &AppError::msg(error));
}

fn record_failure(
    asset_id: Option<&str>,
    source: &Path,
    media_type: &str,
    error: &AppError,
) {
    let entry = ThumbnailFailure {
        asset_id: asset_id.map(str::to_string),
        path: source.display().to_string(),
        media_type: media_type.to_string(),
        error: error.to_string(),
        at: chrono::Utc::now().to_rfc3339(),
    };
    let mut q = RECENT_FAILURES.lock();
    if q.len() >= MAX_RECENT_FAILURES {
        q.pop_front();
    }
    q.push_back(entry);
}

pub fn thumbnail_path(thumbs_dir: &Path, hash: &str) -> PathBuf {
    thumbs_dir.join(format!("{hash}{THUMB_SUFFIX}"))
}

fn is_current_thumb(path: &str) -> bool {
    path.ends_with(THUMB_SUFFIX) && Path::new(path).is_file()
}

/// Generate an image or video thumbnail with retries and failure logging.
pub fn generate_media_thumbnail_with_retry(
    source: &Path,
    thumbs_dir: &Path,
    hash: &str,
    media_type: &str,
    asset_id: Option<&str>,
) -> AppResult<PathBuf> {
    // External converts (HEIC / RAW) are deterministic — one attempt.
    let attempts = if media_type != "video"
        && (heic::is_heic_path(source) || raw::is_raw_path(source))
    {
        1
    } else {
        THUMB_ATTEMPTS
    };
    let mut last_err: Option<AppError> = None;
    for attempt in 1..=attempts {
        let result = if media_type == "video" {
            ffmpeg::extract_frame_thumbnail(source, thumbs_dir, hash)
        } else {
            generate_thumbnail(source, thumbs_dir, hash)
        };
        match result {
            Ok(dest) => {
                if attempt > 1 {
                    tracing::info!(
                        attempt,
                        path = %source.display(),
                        media_type,
                        "thumbnail succeeded after retry"
                    );
                }
                return Ok(dest);
            }
            Err(e) => {
                tracing::warn!(
                    attempt,
                    attempts,
                    path = %source.display(),
                    media_type,
                    error = %e,
                    "thumbnail attempt failed"
                );
                let dest = thumbnail_path(thumbs_dir, hash);
                let _ = std::fs::remove_file(&dest);
                last_err = Some(e);
                if attempt < attempts {
                    let delay = THUMB_BACKOFF_MS[(attempt as usize) - 1];
                    std::thread::sleep(Duration::from_millis(delay));
                }
            }
        }
    }
    let err = last_err.unwrap_or_else(|| AppError::msg("thumbnail generation failed"));
    tracing::error!(
        path = %source.display(),
        media_type,
        asset_id = asset_id.unwrap_or(""),
        error = %err,
        "thumbnail generation failed after retries"
    );
    record_failure(asset_id, source, media_type, &err);
    Err(err)
}

/// Write a decoded image to a JPEG thumb path with retries.
pub fn write_thumbnail_jpeg_with_retry(
    img: &DynamicImage,
    dest: &Path,
    source: &Path,
    asset_id: Option<&str>,
) -> AppResult<()> {
    let mut last_err: Option<AppError> = None;
    for attempt in 1..=THUMB_ATTEMPTS {
        match write_thumbnail_jpeg(img, dest) {
            Ok(()) => {
                if attempt > 1 {
                    tracing::info!(
                        attempt,
                        path = %source.display(),
                        "thumbnail write succeeded after retry"
                    );
                }
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    attempt,
                    attempts = THUMB_ATTEMPTS,
                    path = %source.display(),
                    error = %e,
                    "thumbnail write attempt failed"
                );
                let _ = std::fs::remove_file(dest);
                last_err = Some(e);
                if attempt < THUMB_ATTEMPTS {
                    let delay = THUMB_BACKOFF_MS[(attempt as usize) - 1];
                    std::thread::sleep(Duration::from_millis(delay));
                }
            }
        }
    }
    let err = last_err.unwrap_or_else(|| AppError::msg("thumbnail write failed"));
    tracing::error!(
        path = %source.display(),
        media_type = "image",
        asset_id = asset_id.unwrap_or(""),
        error = %err,
        "thumbnail write failed after retries"
    );
    record_failure(asset_id, source, "image", &err);
    Err(err)
}

/// Open an image and bake EXIF/TIFF orientation into the pixel buffer.
///
/// Browsers apply orientation when showing the original; `image::open` does
/// not, which made library thumbnails appear rotated relative to the viewer.
///
/// HEIC/HEIF and camera RAW (DNG, …) are decoded via system tools because the
/// `image` crate cannot handle those formats.
pub fn open_oriented(path: &Path) -> AppResult<DynamicImage> {
    if heic::is_heic_path(path) {
        return heic::open_heic(path);
    }
    if raw::is_raw_path(path) {
        return raw::open_raw(path);
    }
    match open_oriented_native(path) {
        Ok(img) => Ok(img),
        Err(e) => {
            // Misnamed / unusual containers: try external decoders.
            let msg = e.to_string().to_ascii_lowercase();
            if msg.contains("not recognized")
                || msg.contains("unsupported")
                || msg.contains("tiff")
            {
                if let Ok(img) = heic::open_heic(path) {
                    return Ok(img);
                }
                if let Ok(img) = raw::open_raw(path) {
                    return Ok(img);
                }
            }
            Err(e)
        }
    }
}

pub(crate) fn open_oriented_native(path: &Path) -> AppResult<DynamicImage> {
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

    // HEIC / RAW: ask sips/ffmpeg to emit the thumb-sized JPEG directly.
    if heic::is_heic_path(source) {
        heic::write_thumbnail(source, &dest, THUMB_MAX_EDGE)?;
        return Ok(dest);
    }
    if raw::is_raw_path(source) {
        raw::write_thumbnail(source, &dest, THUMB_MAX_EDGE)?;
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

/// How many new thumbs since the last full WalkDir. Import writes hundreds of
/// thumbs; walking the cache after each one dominated scan time.
static THUMBS_SINCE_ENFORCE: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

const ENFORCE_EVERY_N_THUMBS: u32 = 64;

/// Record a newly written thumb; WalkDir only every [`ENFORCE_EVERY_N_THUMBS`].
pub fn note_thumb_written(thumbs_dir: &Path, budget_mb: u32) {
    let n = THUMBS_SINCE_ENFORCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    if n >= ENFORCE_EVERY_N_THUMBS {
        flush_cache_budget(thumbs_dir, budget_mb);
    }
}

/// Force a budget pass (call at end of import / explicit cache clear).
pub fn flush_cache_budget(thumbs_dir: &Path, budget_mb: u32) {
    THUMBS_SINCE_ENFORCE.store(0, std::sync::atomic::Ordering::Relaxed);
    enforce_cache_budget(thumbs_dir, budget_mb);
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
    repair_missing_thumbnails_capped(conn, thumbs_dir, usize::MAX)
}

/// Regen at most `limit` missing thumbs (startup uses a small cap so boot stays light).
pub fn repair_missing_thumbnails_capped(
    conn: &rusqlite::Connection,
    thumbs_dir: &Path,
    limit: usize,
) -> AppResult<u32> {
    repair_missing_thumbnails_with_progress(conn, thumbs_dir, limit, |_| {})
}

/// Same as [`repair_missing_thumbnails`], with a progress callback after each attempt.
pub fn repair_missing_thumbnails_with_progress(
    conn: &rusqlite::Connection,
    thumbs_dir: &Path,
    limit: usize,
    mut on_progress: impl FnMut(crate::models::ThumbnailRepairProgress),
) -> AppResult<u32> {
    use crate::models::ThumbnailRepairProgress;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;

    // Materialize candidates first so we don't hold an open SELECT while doing
    // slow ffmpeg/image work and UPDATEs (that pattern races other writers).
    let candidates: Vec<(String, String, String, Option<String>, String)> = {
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
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut work: Vec<(String, String, String, Option<String>, String)> = Vec::new();
    for (id, path, hash, thumb_path, media_type) in candidates {
        let needs = match &thumb_path {
            Some(p) => !is_current_thumb(p),
            None => true,
        };
        if !needs {
            continue;
        }
        if !Path::new(&path).is_file() {
            continue;
        }
        work.push((id, path, hash, thumb_path, media_type));
        if work.len() >= limit {
            break;
        }
    }

    let total = work.len() as u32;
    on_progress(ThumbnailRepairProgress {
        phase: "repairing".into(),
        op: String::new(),
        current: 0,
        total,
        repaired: 0,
        path: None,
    });

    if work.is_empty() {
        on_progress(ThumbnailRepairProgress {
            phase: "done".into(),
            op: String::new(),
            current: 0,
            total: 0,
            repaired: 0,
            path: None,
        });
        return Ok(0);
    }

    // Cheap cleanup before parallel convert (avoids races on legacy siblings).
    for (_, _, hash, thumb_path, _) in &work {
        if let Some(old) = thumb_path {
            if !old.ends_with(THUMB_SUFFIX) {
                let _ = std::fs::remove_file(old);
            }
        }
        let legacy = thumbs_dir.join(format!("{hash}.jpg"));
        if legacy.exists() {
            let _ = std::fs::remove_file(&legacy);
        }
        let dest = thumbnail_path(thumbs_dir, hash);
        let _ = std::fs::remove_file(&dest);
    }

    // ffmpeg HEIC converts are CPU-light relative to full decode; allow more
    // concurrency so phone libraries finish repair faster.
    let workers = std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8))
        .unwrap_or(4);
    let work = Arc::new(work);
    let next = Arc::new(AtomicUsize::new(0));
    let thumbs_dir_buf = thumbs_dir.to_path_buf();
    let (tx, rx) = mpsc::channel::<(usize, String, String, AppResult<PathBuf>)>();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let work = Arc::clone(&work);
            let next = Arc::clone(&next);
            let thumbs_dir_buf = thumbs_dir_buf.clone();
            let tx = tx.clone();
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= work.len() {
                        break;
                    }
                    let (id, path, hash, _thumb_path, media_type) = &work[i];
                    let result = generate_media_thumbnail_with_retry(
                        Path::new(path),
                        &thumbs_dir_buf,
                        hash,
                        media_type,
                        Some(id),
                    );
                    let _ = tx.send((i, id.clone(), path.clone(), result));
                }
            });
        }
        drop(tx);

        let mut completed = 0u32;
        let mut repaired = 0u32;
        for (_i, id, path, result) in rx {
            completed = completed.saturating_add(1);
            on_progress(ThumbnailRepairProgress {
                phase: "repairing".into(),
                op: String::new(),
                current: completed,
                total,
                repaired,
                path: Some(path.clone()),
            });
            match result {
                Ok(dest) => {
                    let dest_str = dest.to_string_lossy().to_string();
                    match update_thumbnail_path(conn, &id, &dest_str) {
                        Ok(()) => repaired = repaired.saturating_add(1),
                        Err(e) => {
                            tracing::error!(
                                asset_id = %id,
                                path = %path,
                                error = %e,
                                "thumbnail path update failed after generate"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        asset_id = %id,
                        path = %path,
                        error = %e,
                        "thumbnail repair failed"
                    );
                }
            }
        }

        on_progress(ThumbnailRepairProgress {
            phase: "done".into(),
            op: String::new(),
            current: total,
            total,
            repaired,
            path: None,
        });
        Ok(repaired)
    })
}

fn update_thumbnail_path(conn: &rusqlite::Connection, id: &str, dest: &str) -> AppResult<()> {
    const ATTEMPTS: u32 = 8;
    let mut last = None;
    for attempt in 1..=ATTEMPTS {
        match conn.execute(
            "UPDATE assets SET thumbnail_path = ?1 WHERE id = ?2",
            rusqlite::params![dest, id],
        ) {
            Ok(_) => return Ok(()),
            Err(e) => {
                let err = AppError::from(e);
                if err.is_db_busy() && attempt < ATTEMPTS {
                    tracing::warn!(
                        attempt,
                        asset_id = %id,
                        "thumbnail db update busy; retrying"
                    );
                    std::thread::sleep(Duration::from_millis(50u64.saturating_mul(attempt as u64)));
                    last = Some(err);
                    continue;
                }
                return Err(err);
            }
        }
    }
    Err(last.unwrap_or_else(|| AppError::msg("thumbnail path update failed")))
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

    #[test]
    fn records_thumbnail_failures_in_ring_buffer() {
        record_failure_for_path(
            Some("asset-1"),
            Path::new("/tmp/missing.mp4"),
            "video",
            "ffmpeg not found on PATH",
        );
        let failures = recent_failures();
        assert!(!failures.is_empty());
        let last = failures.last().unwrap();
        assert_eq!(last.asset_id.as_deref(), Some("asset-1"));
        assert_eq!(last.media_type, "video");
        assert!(last.error.contains("ffmpeg"));
    }
}
