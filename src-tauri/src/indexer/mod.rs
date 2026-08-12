pub mod queue;
pub mod scan;

use std::path::Path;

use chrono::{DateTime, Utc};
use image::GenericImageView;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::ImportResult;
use crate::thumbnails;

pub use scan::{is_indexable_media, is_supported_media, media_type_for_path, MediaKind};

/// True when the on-disk file matches the indexed row's size and was not
/// modified after `indexed_at`. Avoids SHA256 + decode for unchanged files.
pub fn asset_looks_unchanged(conn: &Connection, path: &Path) -> AppResult<bool> {
    let path_str = path.to_string_lossy();
    let row: Option<(Option<i64>, String)> = conn
        .query_row(
            "SELECT file_size, indexed_at FROM assets
             WHERE path = ?1 AND deleted_at IS NULL",
            params![path_str.as_ref()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((size, indexed_at)) = row else {
        return Ok(false);
    };
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };
    if size.map(|s| s as u64) != Some(meta.len()) {
        return Ok(false);
    }
    let Ok(modified) = meta.modified() else {
        return Ok(false);
    };
    let Ok(indexed) = DateTime::parse_from_rfc3339(&indexed_at) else {
        return Ok(false);
    };
    let indexed: std::time::SystemTime = indexed.with_timezone(&Utc).into();
    Ok(modified <= indexed)
}

/// Path → (file_size, indexed_at) for auto-scan skip checks.
pub fn indexed_file_fingerprints(
    conn: &Connection,
) -> AppResult<std::collections::HashMap<String, (Option<i64>, String)>> {
    let mut stmt = conn.prepare(
        "SELECT path, file_size, indexed_at FROM assets WHERE deleted_at IS NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?))
    })?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let (path, size, indexed_at) = row?;
        out.insert(path, (size, indexed_at));
    }
    Ok(out)
}

/// Same check as [`asset_looks_unchanged`] without a DB round-trip.
pub fn fingerprint_matches_disk(
    path: &Path,
    size: Option<i64>,
    indexed_at: &str,
) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if size.map(|s| s as u64) != Some(meta.len()) {
        return false;
    }
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(indexed) = DateTime::parse_from_rfc3339(indexed_at) else {
        return false;
    };
    let indexed: std::time::SystemTime = indexed.with_timezone(&Utc).into();
    modified <= indexed
}

/// Options applied while scanning / preparing assets for import.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub ignore_patterns: Vec<String>,
    pub preserve_exif: bool,
    pub thumbnail_cache_mb: u32,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            ignore_patterns: Vec::new(),
            preserve_exif: true,
            thumbnail_cache_mb: 1024,
        }
    }
}

impl ImportOptions {
    pub fn from_prefs(prefs: &crate::preferences::Preferences) -> Self {
        Self {
            ignore_patterns: prefs.library.ignore_patterns.clone(),
            preserve_exif: prefs.privacy.preserve_exif,
            thumbnail_cache_mb: prefs.performance.thumbnail_cache_mb,
        }
    }
}

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
    if asset_looks_unchanged(conn, path)? {
        return Ok(UpsertOutcome::Skipped);
    }
    let Some(prepared) =
        prepare_asset(path, thumbs_dir, generate_thumb, &ImportOptions::default())?
    else {
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
    options: &ImportOptions,
) -> AppResult<Option<PreparedAsset>> {
    if !path.is_file() {
        return Ok(None);
    }
    if crate::prefs_runtime::path_is_ignored(path, &options.ignore_patterns) {
        return Ok(None);
    }
    let Some(kind) = media_type_for_path(path) else {
        return Ok(None);
    };

    let hash = sha256_file(path)?;
    let meta = read_media_meta(path, kind, thumbs_dir, generate_thumb, &hash, options)?;
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
             blur_score=?12,
             captured_ym = strftime('%Y-%m', COALESCE(?8, created_at)),
             captured_md = strftime('%m-%d', COALESCE(?8, created_at)),
             deleted_at=NULL WHERE id=?13",
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
                created_at, captured_at, indexed_at, camera, lens, blur_score, captured_ym, captured_md
             ) VALUES (
                ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                strftime('%Y-%m', COALESCE(?11, ?10)),
                strftime('%m-%d', COALESCE(?11, ?10))
             )",
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

    refresh_fts_basic(
        conn,
        &id,
        &path_str,
        meta.camera.as_deref(),
        meta.lens.as_deref(),
    )?;
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
        &ImportOptions::default(),
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
    options: &ImportOptions,
    mut on_progress: impl FnMut(u64, u64, &Path),
) -> AppResult<ImportResult> {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::thread;

    let files = collect_media_files_filtered(roots, &options.ignore_patterns)?;
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
    let options = Arc::new(options.clone());

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let files = Arc::clone(&files);
        let next = Arc::clone(&next);
        let stop = Arc::clone(&stop);
        let cancel = Arc::clone(&cancel);
        let thumbs_dir = thumbs_dir.clone();
        let options = Arc::clone(&options);
        let tx = tx.clone();
        handles.push(thread::spawn(move || loop {
            if stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                stop.store(true, Ordering::Relaxed);
                break;
            }
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= files.len() {
                break;
            }
            let prepared = prepare_asset(&files[i], &thumbs_dir, true, &options);
            if tx.send((i, prepared)).is_err() {
                break;
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
#[allow(dead_code)]
pub fn collect_media_files(roots: &[std::path::PathBuf]) -> AppResult<Vec<std::path::PathBuf>> {
    collect_media_files_filtered(roots, &[])
}

/// Like [`collect_media_files`], but skips paths matching `ignore_patterns`.
pub fn collect_media_files_filtered(
    roots: &[std::path::PathBuf],
    ignore_patterns: &[String],
) -> AppResult<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in roots {
        if root.is_file() {
            if !is_indexable_media(root, ignore_patterns) {
                if is_supported_media(root) {
                    // Explicitly selected but ignored — skip quietly.
                    continue;
                }
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
                if path.is_file() && is_indexable_media(&path, ignore_patterns) {
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

fn fill_external_image_meta(
    meta: &mut MediaMeta,
    path: &Path,
    thumbs_dir: &Path,
    generate_thumb: bool,
    hash: &str,
    options: &ImportOptions,
    kind: &str,
    write_thumb: fn(&Path, &Path, u32) -> AppResult<()>,
    open_scaled: fn(&Path, u32) -> AppResult<image::DynamicImage>,
) {
    if let Some((w, h)) = thumbnails::heic::probe_dimensions(path) {
        meta.width = Some(w as i64);
        meta.height = Some(h as i64);
    }

    let dest = thumbnails::thumbnail_path(thumbs_dir, hash);
    let thumb_img = if generate_thumb {
        let ready = if dest.exists() {
            true
        } else {
            match write_thumb(path, &dest, thumbnails::THUMB_MAX_EDGE) {
                Ok(()) => {
                    thumbnails::enforce_cache_budget(thumbs_dir, options.thumbnail_cache_mb);
                    true
                }
                Err(e) => {
                    tracing::error!(
                        path = %path.display(),
                        media_type = "image",
                        kind,
                        error = %e,
                        "external thumbnail generation failed"
                    );
                    thumbnails::record_failure_for_path(None, path, "image", &e.to_string());
                    false
                }
            }
        };
        if ready {
            meta.thumbnail_path = Some(dest.clone());
            thumbnails::open_oriented(&dest).ok()
        } else {
            None
        }
    } else {
        open_scaled(path, thumbnails::THUMB_MAX_EDGE).ok()
    };

    match thumb_img {
        Some(img) => {
            if meta.width.is_none() || meta.height.is_none() {
                let (w, h) = img.dimensions();
                meta.width = Some(w as i64);
                meta.height = Some(h as i64);
            }
            meta.perceptual_hash = Some(thumbnails::perceptual_hash_from_image(&img));
            meta.blur_score = Some(crate::blur::blur_score_from_image(&img));
        }
        None if !generate_thumb => {
            tracing::warn!(path = %path.display(), kind, "external image decode skipped");
        }
        None => {}
    }
}

fn read_media_meta(
    path: &Path,
    kind: MediaKind,
    thumbs_dir: &Path,
    generate_thumb: bool,
    hash: &str,
    options: &ImportOptions,
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
        if options.preserve_exif {
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
        }

        // HEIC / RAW: convert once at thumb size via system tools.
        if thumbnails::heic::is_heic_path(path) {
            fill_external_image_meta(
                &mut meta,
                path,
                thumbs_dir,
                generate_thumb,
                hash,
                options,
                "HEIC",
                thumbnails::heic::write_thumbnail,
                thumbnails::heic::open_heic_scaled,
            );
        } else if thumbnails::raw::is_raw_path(path) {
            fill_external_image_meta(
                &mut meta,
                path,
                thumbs_dir,
                generate_thumb,
                hash,
                options,
                "RAW",
                thumbnails::raw::write_thumbnail,
                thumbnails::raw::open_raw_scaled,
            );
        } else {
            // One decode: dimensions + perceptual hash + thumbnail-sized blur + JPEG thumb.
            // Resize once to thumb max edge, then score blur on that (not full-res).
            match thumbnails::open_oriented(path) {
                Ok(img) => {
                    let (w, h) = img.dimensions();
                    meta.width = Some(w as i64);
                    meta.height = Some(h as i64);
                    meta.perceptual_hash = Some(thumbnails::perceptual_hash_from_image(&img));

                    let thumb_src =
                        if w > thumbnails::THUMB_MAX_EDGE || h > thumbnails::THUMB_MAX_EDGE {
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
                        } else if thumbnails::write_thumbnail_jpeg_with_retry(
                            &thumb_src, &dest, path, None,
                        )
                        .is_ok()
                        {
                            meta.thumbnail_path = Some(dest);
                            thumbnails::enforce_cache_budget(
                                thumbs_dir,
                                options.thumbnail_cache_mb,
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "image decode skipped");
                    if generate_thumb {
                        tracing::error!(
                            path = %path.display(),
                            media_type = "image",
                            error = %e,
                            "thumbnail generation failed: image decode error"
                        );
                        thumbnails::record_failure_for_path(None, path, "image", &e.to_string());
                    }
                    if let Ok(dims) = image::image_dimensions(path) {
                        meta.width = Some(dims.0 as i64);
                        meta.height = Some(dims.1 as i64);
                    }
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
                match thumbnails::generate_media_thumbnail_with_retry(
                    path, thumbs_dir, hash, "video", None,
                ) {
                    Ok(dest) => meta.thumbnail_path = Some(dest),
                    Err(_) => {
                        // Already logged + recorded by generate_media_thumbnail_with_retry.
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
        "INSERT INTO assets_fts (asset_id, filename, tags, camera, lens, ocr_text, people, auto_tags, caption)
         VALUES (?1,?2,'',?3,?4,'','','','')",
        params![
            asset_id,
            filename,
            camera.unwrap_or_default(),
            lens.unwrap_or_default()
        ],
    )?;
    Ok(())
}

/// `(path, camera, lens, tag names, ocr, people, auto_tags, caption)` — kept for call-site docs.
pub fn refresh_fts(conn: &Connection, asset_id: &str) -> AppResult<()> {
    let row: Option<(
        String,
        Option<String>,
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
                 FROM asset_labels l WHERE l.asset_id = assets.id),
                (SELECT caption FROM asset_captions WHERE asset_id = assets.id)
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
                    r.get(7)?,
                ))
            },
        )
        .ok();

    conn.execute(
        "DELETE FROM assets_fts WHERE asset_id = ?1",
        params![asset_id],
    )?;

    if let Some((path, camera, lens, tags, ocr_text, people, auto_tags, caption)) = row {
        let filename = Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        conn.execute(
            "INSERT INTO assets_fts (asset_id, filename, tags, camera, lens, ocr_text, people, auto_tags, caption)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                asset_id,
                filename,
                tags.unwrap_or_default(),
                camera.unwrap_or_default(),
                lens.unwrap_or_default(),
                ocr_text.unwrap_or_default(),
                people.unwrap_or_default(),
                auto_tags.unwrap_or_default(),
                caption.unwrap_or_default()
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
    fn upsert_skips_unchanged_file_without_rehash() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&media).unwrap();
        let file = media.join("same.png");
        RgbImage::from_pixel(16, 16, Rgb([4, 5, 6]))
            .save(&file)
            .unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        assert!(matches!(
            upsert_asset(&conn, &file, &thumbs, true).unwrap(),
            UpsertOutcome::Inserted
        ));
        assert!(asset_looks_unchanged(&conn, &file).unwrap());
        assert!(matches!(
            upsert_asset(&conn, &file, &thumbs, true).unwrap(),
            UpsertOutcome::Skipped
        ));
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
            &ImportOptions::default(),
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
            &ImportOptions::default(),
            |_, _, _| {},
        )
        .unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn collect_skips_appledouble_sidecars() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        let img = RgbImage::from_pixel(16, 16, Rgb([1, 2, 3]));
        img.save(media.join("photo.jpg")).unwrap();
        std::fs::write(media.join("._photo.jpg"), [0u8; 4096]).unwrap();

        let files = collect_media_files(&[media]).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("photo.jpg"));
        assert!(!files[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("._"));
    }
}
