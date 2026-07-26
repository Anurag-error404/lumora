use std::collections::HashMap;
use std::fs::File;
use std::io::{copy, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

use crate::error::{AppError, AppResult};
use crate::models::ExportResult;

pub fn export_assets_to_zip(
    conn: &Connection,
    asset_ids: &[String],
    dest: &Path,
) -> AppResult<ExportResult> {
    if asset_ids.is_empty() {
        return Err(AppError::msg(
            "select at least one photo or video to export",
        ));
    }
    if dest.as_os_str().is_empty() {
        return Err(AppError::msg("export path required"));
    }

    let mut paths: Vec<(String, PathBuf)> = Vec::with_capacity(asset_ids.len());
    for id in asset_ids {
        let path: Option<String> = conn
            .query_row(
                "SELECT path FROM assets WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |row| row.get(0),
            )
            .ok();
        if let Some(path) = path {
            paths.push((id.clone(), PathBuf::from(path)));
        }
    }

    if paths.is_empty() {
        return Err(AppError::msg(
            "none of the selected items could be exported",
        ));
    }

    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file = File::create(dest)?;
    let mut zip = ZipWriter::new(file);
    // Media formats are already compressed; Stored keeps export fast.
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    let mut used_names: HashMap<String, usize> = HashMap::new();
    let mut exported = 0u32;
    let mut missing = 0u32;
    let mut errors: Vec<String> = Vec::new();

    for (_id, path) in &paths {
        if !path.is_file() {
            missing += 1;
            errors.push(format!("missing file: {}", path.display()));
            continue;
        }

        let entry_name = unique_entry_name(path, &mut used_names);
        match File::open(path) {
            Ok(src) => {
                if let Err(e) = zip.start_file(&entry_name, options) {
                    errors.push(format!("{}: {e}", path.display()));
                    continue;
                }
                let mut reader = BufReader::new(src);
                if let Err(e) = copy(&mut reader, &mut zip) {
                    errors.push(format!("{}: {e}", path.display()));
                    continue;
                }
                exported += 1;
            }
            Err(e) => {
                missing += 1;
                errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    zip.finish()?;

    if exported == 0 {
        let _ = std::fs::remove_file(dest);
        return Err(AppError::msg(
            errors
                .first()
                .cloned()
                .unwrap_or_else(|| "no files were exported".into()),
        ));
    }

    Ok(ExportResult {
        path: dest.display().to_string(),
        exported,
        missing,
        errors,
    })
}

fn unique_entry_name(path: &Path, used_names: &mut HashMap<String, usize>) -> String {
    let base = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());

    let count = used_names.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let ext = path
            .extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();
        format!("{stem}-{count}{ext}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use image::{Rgb, RgbImage};
    use std::io::Read;
    use tempfile::tempdir;
    use zip::ZipArchive;

    fn seed_asset(conn: &Connection, id: &str, path: &Path) {
        conn.execute(
            "INSERT INTO assets (
                id, path, hash, media_type, created_at, indexed_at, favorite, rating
             ) VALUES (?1, ?2, ?3, 'image', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 0, 0)",
            params![id, path.display().to_string(), format!("hash-{id}")],
        )
        .unwrap();
    }

    #[test]
    fn export_selected_assets_to_zip_with_unique_names() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir_all(media.join("a")).unwrap();
        std::fs::create_dir_all(media.join("b")).unwrap();

        RgbImage::from_pixel(8, 8, Rgb([1, 2, 3]))
            .save(media.join("a/photo.jpg"))
            .unwrap();
        RgbImage::from_pixel(8, 8, Rgb([4, 5, 6]))
            .save(media.join("b/photo.jpg"))
            .unwrap();
        RgbImage::from_pixel(8, 8, Rgb([7, 8, 9]))
            .save(media.join("extra.png"))
            .unwrap();

        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "1", &media.join("a/photo.jpg"));
        seed_asset(&conn, "2", &media.join("b/photo.jpg"));
        seed_asset(&conn, "3", &media.join("extra.png"));

        let dest = dir.path().join("share.zip");
        let result = export_assets_to_zip(&conn, &["1".into(), "2".into()], &dest).unwrap();

        assert_eq!(result.exported, 2);
        assert_eq!(result.missing, 0);
        assert!(dest.is_file());

        let mut archive = ZipArchive::new(File::open(&dest).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec!["photo-2.jpg".to_string(), "photo.jpg".to_string()]
        );

        let mut buf = Vec::new();
        archive
            .by_name("photo.jpg")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn export_rejects_empty_selection() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let err = export_assets_to_zip(&conn, &[], &dir.path().join("out.zip")).unwrap_err();
        assert!(err.to_string().contains("select at least one"));
    }
}
