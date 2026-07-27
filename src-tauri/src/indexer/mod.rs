pub mod queue;
pub mod scan;

use std::path::Path;

use chrono::{DateTime, Utc};
use image::GenericImageView;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::ImportResult;
use crate::thumbnails;

pub use scan::{is_supported_media, media_type_for_path, MediaKind};

pub fn sha256_file(path: &Path) -> AppResult<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn upsert_asset(
    conn: &Connection,
    path: &Path,
    thumbs_dir: &Path,
    generate_thumb: bool,
) -> AppResult<UpsertOutcome> {
    let Some(prepared) = prepare_asset(path, thumbs_dir, generate_thumb)? else {
        return Ok(UpsertOutcome::Skipped);
    };
    commit_prepared(conn, &prepared, false)
}

/// CPU-bound work for one file: hash, EXIF, single image decode → thumb + phash.
/// Safe to run on a worker thread with no DB connection held.
pub fn prepare_asset(
    path: &Path,
    thumbs_dir: &Path,
    generate_thumb: bool,
) -> AppResult<Option<PreparedAsset>> {
    if !path.is_file() {
        return Ok(None);
    }
    let Some(kind) = media_type_for_path(path) else {
        return Ok(None);
    };

    let hash = sha256_file(path)?;
    let meta = read_media_meta(path, kind, thumbs_dir, generate_thumb, &hash)?;
    Ok(Some(PreparedAsset {
        path: path.to_path_buf(),
        hash,
        kind,
        meta,
    }))
}

pub fn commit_prepared(
    conn: &Connection,
    prepared: &PreparedAsset,
    skip_content_dupes: bool,
) -> AppResult<UpsertOutcome> {
    let path_str = prepared.path.to_string_lossy().to_string();
    let now = Utc::now().to_rfc3339();
    let meta = &prepared.meta;
    let hash = &prepared.hash;
    let kind = prepared.kind;

    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, hash FROM assets WHERE path = ?1",
            params![path_str],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let (id, outcome) = if let Some((id, old_hash)) = existing {
        if old_hash.as_deref() == Some(hash.as_str()) && !meta.force_write {
            return Ok(UpsertOutcome::Skipped);
        }
        conn.execute(
            "UPDATE assets SET hash=?1, perceptual_hash=?2, media_type=?3, width=?4, height=?5,
             duration_ms=?6, file_size=?7, captured_at=?8, indexed_at=?9, camera=?10, lens=?11,
             blur_score=?12, deleted_at=NULL WHERE id=?13",
            params![
                hash,
                meta.perceptual_hash,
                kind.as_str(),
                meta.width,
                meta.height,
                meta.duration_ms,
                meta.file_size,
                meta.captured_at,
                now,
                meta.camera,
                meta.lens,
                meta.blur_score,
                id
            ],
        )?;
        (id, UpsertOutcome::Updated)
    } else {
        if skip_content_dupes {
            let hash_exists: bool = conn
                .query_row(
                    "SELECT 1 FROM assets WHERE hash = ?1 AND deleted_at IS NULL LIMIT 1",
                    params![hash],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if hash_exists {
                return Ok(UpsertOutcome::Skipped);
            }
        }
        let id = Uuid::new_v4().to_string();
        let created_at = file_created_at(&prepared.path).unwrap_or_else(|| now.clone());
        conn.execute(
            "INSERT INTO assets (
                id, path, hash, perceptual_hash, media_type, width, height, duration_ms, file_size,
                created_at, captured_at, indexed_at, camera, lens, blur_score
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                id,
                path_str,
                hash,
                meta.perceptual_hash,
                kind.as_str(),
                meta.width,
                meta.height,
                meta.duration_ms,
                meta.file_size,
                created_at,
                meta.captured_at,
                now,
                meta.camera,
                meta.lens,
                meta.blur_score
            ],
        )?;
        (id, UpsertOutcome::Inserted)
    };

    if let Some(ref thumb) = meta.thumbnail_path {
        let thumb_str = thumb.to_string_lossy().to_string();
        conn.execute(
            "UPDATE assets SET thumbnail_path=?1 WHERE id=?2",
            params![thumb_str, id],
        )?;
    }

    refresh_fts_basic(conn, &id, &path_str, meta.camera.as_deref(), meta.lens.as_deref())?;
    Ok(outcome)
}

pub fn remove_asset_by_path(conn: &Connection, path: &Path) -> AppResult<bool> {
    let path_str = path.to_string_lossy().to_string();
    let id: Option<String> = conn
        .query_row(
            "SELECT id FROM assets WHERE path = ?1",
            params![path_str],
            |row| row.get(0),
        )
        .ok();
    if let Some(id) = id {
        conn.execute(
            "UPDATE assets SET deleted_at=?1 WHERE id=?2",
            params![Utc::now().to_rfc3339(), id],
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
pub fn import_folder_with_progress(
    conn: &Connection,
    root: &Path,
    thumbs_dir: &Path,
    on_progress: impl FnMut(u64, u64, &Path),
) -> AppResult<ImportResult> {
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    import_paths_with_progress(
        conn,
        &[root.to_path_buf()],
        thumbs_dir,
        Arc::new(AtomicBool::new(false)),
        false,
        on_progress,
    )
}

/// Import media with cancellation. CPU work (hash / decode / thumb) runs on a
/// small thread pool; SQLite writes stay on this thread.
pub fn import_paths_with_progress(
    conn: &Connection,
    roots: &[std::path::PathBuf],
    thumbs_dir: &Path,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    skip_content_dupes: bool,
    mut on_progress: impl FnMut(u64, u64, &Path),
) -> AppResult<ImportResult> {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread;

    let files = collect_media_files(roots)?;
    let total = files.len() as u64;
    let mut scanned = 0u64;
    let mut inserted = 0u64;
    let mut updated = 0u64;
    let mut skipped = 0u64;
    let mut cancelled = false;
    let started = std::time::Instant::now();

    if files.is_empty() {
        return Ok(ImportResult {
            scanned: 0,
            inserted: 0,
            updated: 0,
            skipped: 0,
            cancelled: false,
            duration_ms: 0,
            files_per_sec: 0.0,
        });
    }

    let workers = thread::available_parallelism()
        .map(|n| n.get().clamp(2, 16))
        .unwrap_or(4);
    let files = Arc::new(files);
    let thumbs_dir = thumbs_dir.to_path_buf();
    let next = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<(usize, AppResult<Option<PreparedAsset>>)>();

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let files = Arc::clone(&files);
        let next = Arc::clone(&next);
        let stop = Arc::clone(&stop);
        let cancel = Arc::clone(&cancel);
        let thumbs_dir = thumbs_dir.clone();
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            loop {
                if stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= files.len() {
                    break;
                }
                let prepared = prepare_asset(&files[i], &thumbs_dir, true);
                if tx.send((i, prepared)).is_err() {
                    break;
                }
            }
        }));
    }
    drop(tx);

    // Batch SQLite writes — per-file auto-commit was a major serial bottleneck.
    const COMMIT_EVERY: usize = 64;
    let _ = conn.execute_batch("BEGIN IMMEDIATE");
    let mut since_commit = 0usize;

    let mut received = 0usize;
    while received < files.len() {
        if cancel.load(Ordering::Relaxed) {
            stop.store(true, Ordering::Relaxed);
            cancelled = true;
            while rx.recv().is_ok() {
                received += 1;
                if received >= files.len() {
                    break;
                }
            }
            break;
        }

        let Ok((index, prepared)) = rx.recv() else {
            break;
        };
        received += 1;
        scanned += 1;
        let path = &files[index];
        on_progress(scanned, total, path);

        match prepared {
            Ok(Some(asset)) => match commit_prepared(conn, &asset, skip_content_dupes) {
                Ok(UpsertOutcome::Inserted) => {
                    inserted += 1;
                    since_commit += 1;
                }
                Ok(UpsertOutcome::Updated) => {
                    updated += 1;
                    since_commit += 1;
                }
                Ok(UpsertOutcome::Skipped) => skipped += 1,
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "import commit failed");
                    skipped += 1;
                }
            },
            Ok(None) => skipped += 1,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "import prepare failed");
                skipped += 1;
            }
        }

        if since_commit >= COMMIT_EVERY {
            let _ = conn.execute_batch("COMMIT; BEGIN IMMEDIATE");
            since_commit = 0;
        }
    }

    let _ = conn.execute_batch("COMMIT");

    for handle in handles {
        let _ = handle.join();
    }

    let duration_ms = started.elapsed().as_millis() as u64;
    let files_per_sec = if duration_ms == 0 {
        0.0
    } else {
        (scanned as f64) * 1000.0 / (duration_ms as f64)
    };

    Ok(ImportResult {
        scanned,
        inserted,
        updated,
        skipped,
        cancelled,
        duration_ms,
        files_per_sec,
    })
}

/// Collect supported media files from directories (recursive) and individual file paths.
pub fn collect_media_files(roots: &[std::path::PathBuf]) -> AppResult<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in roots {
        if root.is_file() {
            if !is_supported_media(root) {
                return Err(crate::error::AppError::msg(format!(
                    "unsupported media file: {}",
                    root.display()
                )));
            }
            let key = root.display().to_string();
            if seen.insert(key) {
                files.push(root.clone());
            }
            continue;
        }
        if root.is_dir() {
            for entry in walkdir::WalkDir::new(root)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.into_path();
                if path.is_file() && is_supported_media(&path) {
                    let key = path.display().to_string();
                    if seen.insert(key) {
                        files.push(path);
                    }
                }
            }
            continue;
        }
        return Err(crate::error::AppError::msg(format!(
            "path not found: {}",
            root.display()
        )));
    }

    files.sort();
    Ok(files)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted,
    Updated,
    Skipped,
}

pub struct PreparedAsset {
    path: std::path::PathBuf,
    hash: String,
    kind: MediaKind,
    meta: MediaMeta,
}

struct MediaMeta {
    width: Option<i64>,
    height: Option<i64>,
    duration_ms: Option<i64>,
    file_size: Option<i64>,
    captured_at: Option<String>,
    camera: Option<String>,
    lens: Option<String>,
    perceptual_hash: Option<String>,
    blur_score: Option<f64>,
    thumbnail_path: Option<std::path::PathBuf>,
    /// When true, commit even if the content hash is unchanged (forced thumb regen).
    force_write: bool,
}

fn read_media_meta(
    path: &Path,
    kind: MediaKind,
    thumbs_dir: &Path,
    generate_thumb: bool,
    hash: &str,
) -> AppResult<MediaMeta> {
    let file_size = std::fs::metadata(path).ok().map(|m| m.len() as i64);
    let mut meta = MediaMeta {
        width: None,
        height: None,
        duration_ms: None,
        file_size,
        captured_at: None,
        camera: None,
        lens: None,
        perceptual_hash: None,
        blur_score: None,
        thumbnail_path: None,
        force_write: false,
    };

    if kind == MediaKind::Image {
        if let Ok(file) = std::fs::File::open(path) {
            let mut bufreader = std::io::BufReader::new(&file);
            if let Ok(exif) = exif::Reader::new().read_from_container(&mut bufreader) {
                meta.captured_at = exif
                    .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
                    .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))
                    .map(|f| f.display_value().to_string());
                meta.camera = exif
                    .get_field(exif::Tag::Model, exif::In::PRIMARY)
                    .map(|f| f.display_value().to_string().trim_matches('"').to_string());
                meta.lens = exif
                    .get_field(exif::Tag::LensModel, exif::In::PRIMARY)
                    .map(|f| f.display_value().to_string().trim_matches('"').to_string());
            }
        }

        // One decode: dimensions + perceptual hash + thumbnail-sized blur + JPEG thumb.
        // Resize once to thumb max edge, then score blur on that (not full-res).
        match thumbnails::open_oriented(path) {
            Ok(img) => {
                let (w, h) = img.dimensions();
                meta.width = Some(w as i64);
                meta.height = Some(h as i64);
                meta.perceptual_hash = Some(thumbnails::perceptual_hash_from_image(&img));

                let thumb_src = if w > thumbnails::THUMB_MAX_EDGE || h > thumbnails::THUMB_MAX_EDGE {
                    img.resize(
                        thumbnails::THUMB_MAX_EDGE,
                        thumbnails::THUMB_MAX_EDGE,
                        image::imageops::FilterType::Triangle,
                    )
                } else {
                    img
                };
                meta.blur_score = Some(crate::blur::blur_score_from_image(&thumb_src));
                if generate_thumb {
                    let dest = thumbnails::thumbnail_path(thumbs_dir, hash);
                    if dest.exists() {
                        meta.thumbnail_path = Some(dest);
                    } else if thumbnails::write_thumbnail_jpeg(&thumb_src, &dest).is_ok() {
                        meta.thumbnail_path = Some(dest);
                    }
                }
            }
            Err(e) => {
                tracing::debug!(path = %path.display(), error = %e, "image decode skipped");
                if let Ok(dims) = image::image_dimensions(path) {
                    meta.width = Some(dims.0 as i64);
                    meta.height = Some(dims.1 as i64);
                }
            }
        }
    } else if kind == MediaKind::Video {
        let probe = thumbnails::ffmpeg::probe_video(path);
        meta.width = probe.width;
        meta.height = probe.height;
        meta.duration_ms = probe.duration_ms;
        if generate_thumb {
            let dest = thumbnails::thumbnail_path(thumbs_dir, hash);
            if dest.exists() {
                meta.thumbnail_path = Some(dest);
            } else {
                match thumbnails::ffmpeg::extract_frame_thumbnail(path, thumbs_dir, hash) {
                    Ok(dest) => meta.thumbnail_path = Some(dest),
                    Err(e) => {
                        tracing::debug!(
                            path = %path.display(),
                            error = %e,
                            "video frame thumbnail skipped"
                        );
                    }
                }
            }
        }
    }

    Ok(meta)
}

fn file_created_at(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let datetime: DateTime<Utc> = modified.into();
    Some(datetime.to_rfc3339())
}

/// Fast FTS update for import: filename + camera/lens only (no tag/OCR/people joins).
/// Full `refresh_fts` remains for edits that change derived search fields.
pub fn refresh_fts_basic(
    conn: &Connection,
    asset_id: &str,
    path: &str,
    camera: Option<&str>,
    lens: Option<&str>,
) -> AppResult<()> {
    let filename = Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    conn.execute(
        "DELETE FROM assets_fts WHERE asset_id = ?1",
        params![asset_id],
    )?;
    conn.execute(
        "INSERT INTO assets_fts (asset_id, filename, tags, camera, lens, ocr_text, people, auto_tags)
         VALUES (?1,?2,'',?3,?4,'','','')",
        params![
            asset_id,
            filename,
            camera.unwrap_or_default(),
            lens.unwrap_or_default()
        ],
    )?;
    Ok(())
}

/// `(path, camera, lens, tag names, ocr, people, auto_tags)` — kept for call-site docs.
pub fn refresh_fts(conn: &Connection, asset_id: &str) -> AppResult<()> {
    let row: Option<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = conn
        .query_row(
            "SELECT path, camera, lens,
                (SELECT GROUP_CONCAT(t.name, ' ') FROM asset_tags at
                 JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = assets.id),
                (SELECT text FROM asset_text WHERE asset_id = assets.id),
                (SELECT GROUP_CONCAT(p.name, ' ') FROM faces f
                 JOIN people p ON p.id = f.person_id
                 WHERE f.asset_id = assets.id AND p.ignored = 0
                   AND p.name IS NOT NULL AND TRIM(p.name) != ''),
                (SELECT GROUP_CONCAT(
                    CASE WHEN instr(l.label, ',') > 0
                         THEN trim(substr(l.label, 1, instr(l.label, ',') - 1))
                         ELSE l.label END, ' ')
                 FROM asset_labels l WHERE l.asset_id = assets.id)
             FROM assets WHERE id = ?1",
            params![asset_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .ok();

    conn.execute(
        "DELETE FROM assets_fts WHERE asset_id = ?1",
        params![asset_id],
    )?;

    if let Some((path, camera, lens, tags, ocr_text, people, auto_tags)) = row {
        let filename = Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        conn.execute(
            "INSERT INTO assets_fts (asset_id, filename, tags, camera, lens, ocr_text, people, auto_tags)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                asset_id,
                filename,
                tags.unwrap_or_default(),
                camera.unwrap_or_default(),
                lens.unwrap_or_default(),
                ocr_text.unwrap_or_default(),
                people.unwrap_or_default(),
                auto_tags.unwrap_or_default()
            ],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod import_tests {
    use super::*;
    use crate::db;
    use image::{Rgb, RgbImage};
    use tempfile::tempdir;

    #[test]
    fn import_folder_inserts_and_is_incremental() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&media).unwrap();
        let img = RgbImage::from_pixel(64, 64, Rgb([1, 2, 3]));
        img.save(media.join("a.png")).unwrap();
        img.save(media.join("b.png")).unwrap();

        let db_path = dir.path().join("library.db");
        let conn = db::open_and_migrate(&db_path).unwrap();
        let r1 = import_folder_with_progress(&conn, &media, &thumbs, |_, _, _| {}).unwrap();
        assert_eq!(r1.scanned, 2);
        assert_eq!(r1.inserted, 2);

        let r2 = import_folder_with_progress(&conn, &media, &thumbs, |_, _, _| {}).unwrap();
        assert_eq!(r2.scanned, 2);
        // second pass should skip unchanged hashes when generate_thumb finds existing thumbs
        assert!(r2.inserted == 0);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn import_single_media_file() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&media).unwrap();
        let file = media.join("solo.png");
        RgbImage::from_pixel(32, 32, Rgb([9, 8, 7]))
            .save(&file)
            .unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let result = import_paths_with_progress(
            &conn,
            std::slice::from_ref(&file),
            &thumbs,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            false,
            |_, _, _| {},
        )
        .unwrap();

        assert_eq!(result.scanned, 1);
        assert_eq!(result.inserted, 1);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn import_rejects_unsupported_file() {
        let dir = tempdir().unwrap();
        let notes = dir.path().join("notes.txt");
        std::fs::write(&notes, "hello").unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let err = import_paths_with_progress(
            &conn,
            &[notes],
            &dir.path().join("thumbs"),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            false,
            |_, _, _| {},
        )
        .unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }
}
