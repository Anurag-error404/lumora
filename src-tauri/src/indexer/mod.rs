pub mod queue;
pub mod scan;

use std::path::Path;

use chrono::{DateTime, Utc};
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
    if !path.is_file() {
        return Ok(UpsertOutcome::Skipped);
    }
    let Some(kind) = media_type_for_path(path) else {
        return Ok(UpsertOutcome::Skipped);
    };

    let hash = sha256_file(path)?;
    let path_str = path.to_string_lossy().to_string();
    let now = Utc::now().to_rfc3339();
    let meta = read_media_meta(path, kind)?;

    let existing: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT id, hash FROM assets WHERE path = ?1",
            params![path_str],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let (id, outcome) = if let Some((id, old_hash)) = existing {
        if old_hash.as_deref() == Some(hash.as_str()) && !generate_thumb {
            return Ok(UpsertOutcome::Skipped);
        }
        conn.execute(
            "UPDATE assets SET hash=?1, perceptual_hash=?2, media_type=?3, width=?4, height=?5,
             duration_ms=?6, file_size=?7, captured_at=?8, indexed_at=?9, camera=?10, lens=?11,
             deleted_at=NULL WHERE id=?12",
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
                id
            ],
        )?;
        (id, UpsertOutcome::Updated)
    } else {
        let id = Uuid::new_v4().to_string();
        let created_at = file_created_at(path).unwrap_or_else(|| now.clone());
        conn.execute(
            "INSERT INTO assets (
                id, path, hash, perceptual_hash, media_type, width, height, duration_ms, file_size,
                created_at, captured_at, indexed_at, camera, lens
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
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
                meta.lens
            ],
        )?;
        (id, UpsertOutcome::Inserted)
    };

    if generate_thumb && kind == MediaKind::Image {
        if let Ok(thumb) = thumbnails::generate_thumbnail(path, thumbs_dir, &hash) {
            let thumb_str = thumb.to_string_lossy().to_string();
            conn.execute(
                "UPDATE assets SET thumbnail_path=?1 WHERE id=?2",
                params![thumb_str, id],
            )?;
        }
    }

    refresh_fts(conn, &id)?;
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
    import_paths_with_progress(conn, &[root.to_path_buf()], thumbs_dir, on_progress)
}

pub fn import_paths_with_progress(
    conn: &Connection,
    roots: &[std::path::PathBuf],
    thumbs_dir: &Path,
    mut on_progress: impl FnMut(u64, u64, &Path),
) -> AppResult<ImportResult> {
    let files = collect_media_files(roots)?;
    let total = files.len() as u64;
    let mut scanned = 0u64;
    let mut inserted = 0u64;
    let mut updated = 0u64;
    let mut skipped = 0u64;

    for path in &files {
        scanned += 1;
        on_progress(scanned, total, path);
        match upsert_asset(conn, path, thumbs_dir, true)? {
            UpsertOutcome::Inserted => inserted += 1,
            UpsertOutcome::Updated => updated += 1,
            UpsertOutcome::Skipped => skipped += 1,
        }
    }

    Ok(ImportResult {
        scanned,
        inserted,
        updated,
        skipped,
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

struct MediaMeta {
    width: Option<i64>,
    height: Option<i64>,
    duration_ms: Option<i64>,
    file_size: Option<i64>,
    captured_at: Option<String>,
    camera: Option<String>,
    lens: Option<String>,
    perceptual_hash: Option<String>,
}

fn read_media_meta(path: &Path, kind: MediaKind) -> AppResult<MediaMeta> {
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
    };

    if kind == MediaKind::Image {
        if let Ok(img) = image::image_dimensions(path) {
            meta.width = Some(img.0 as i64);
            meta.height = Some(img.1 as i64);
        }
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
        if let Ok(phash) = thumbnails::perceptual_hash(path) {
            meta.perceptual_hash = Some(phash);
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

/// `(path, camera, lens, tag names)`
type FtsRow = (String, Option<String>, Option<String>, Option<String>);

pub fn refresh_fts(conn: &Connection, asset_id: &str) -> AppResult<()> {
    let row: Option<FtsRow> = conn
        .query_row(
            "SELECT path, camera, lens,
                (SELECT GROUP_CONCAT(t.name, ' ') FROM asset_tags at
                 JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = assets.id)
             FROM assets WHERE id = ?1",
            params![asset_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();

    conn.execute(
        "DELETE FROM assets_fts WHERE asset_id = ?1",
        params![asset_id],
    )?;

    if let Some((path, camera, lens, tags)) = row {
        let filename = Path::new(&path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        conn.execute(
            "INSERT INTO assets_fts (asset_id, filename, tags, camera, lens) VALUES (?1,?2,?3,?4,?5)",
            params![
                asset_id,
                filename,
                tags.unwrap_or_default(),
                camera.unwrap_or_default(),
                lens.unwrap_or_default()
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
        let result =
            import_paths_with_progress(&conn, std::slice::from_ref(&file), &thumbs, |_, _, _| {})
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
        let err =
            import_paths_with_progress(&conn, &[notes], &dir.path().join("thumbs"), |_, _, _| {})
                .unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }
}
