use std::collections::HashMap;
use std::fs::File;
use std::io::{copy, BufReader, Cursor, Write};
use std::path::{Component, Path, PathBuf};

use image::{GenericImageView, ImageFormat};
use rusqlite::{params, Connection};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

use crate::error::{AppError, AppResult};
use crate::models::ExportResult;

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub strip_metadata: bool,
    pub jpeg_quality: u8,
    pub preserve_folder_structure: bool,
    pub max_edge: u32,
    pub naming: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            strip_metadata: false,
            jpeg_quality: 95,
            preserve_folder_structure: true,
            max_edge: 0,
            naming: "original".into(),
        }
    }
}

pub fn export_assets_to_zip(
    conn: &Connection,
    asset_ids: &[String],
    dest: &Path,
    options: ExportOptions,
) -> AppResult<ExportResult> {
    if asset_ids.is_empty() {
        return Err(AppError::msg(
            "select at least one photo or video to export",
        ));
    }
    if dest.as_os_str().is_empty() {
        return Err(AppError::msg("export path required"));
    }

    let mut paths: Vec<(String, PathBuf, Option<String>)> = Vec::with_capacity(asset_ids.len());
    for id in asset_ids {
        let path: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT path, captured_at FROM assets WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();
        if let Some((path, captured_at)) = path {
            paths.push((id.clone(), PathBuf::from(path), captured_at));
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
    let options_zip = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    let mut used_names: HashMap<String, usize> = HashMap::new();
    let common_parent = paths.iter().filter_map(|(_, path, _)| path.parent()).fold(
        None::<PathBuf>,
        |common, parent| {
            Some(match common {
                Some(existing) => common_path(&existing, parent),
                None => parent.to_path_buf(),
            })
        },
    );
    let mut exported = 0u32;
    let mut missing = 0u32;
    let mut errors: Vec<String> = Vec::new();

    for (index, (_id, path, captured_at)) in paths.iter().enumerate() {
        if !path.is_file() {
            missing += 1;
            errors.push(format!("missing file: {}", path.display()));
            continue;
        }

        let entry_name = unique_entry_name(
            path,
            captured_at.as_deref(),
            index + 1,
            common_parent.as_deref(),
            &options,
            &mut used_names,
        );
        if let Err(e) = zip.start_file(&entry_name, options_zip) {
            errors.push(format!("{}: {e}", path.display()));
            continue;
        }

        let write_result = if options.strip_metadata || options.max_edge > 0 {
            match processed_image_bytes(path, options.jpeg_quality, options.max_edge) {
                Ok(Some(bytes)) => zip.write_all(&bytes).map_err(|e| e.to_string()),
                Ok(None) => {
                    // Videos / unsupported: fall back to byte-copy (cannot strip).
                    copy_file_into_zip(path, &mut zip)
                }
                Err(e) => Err(e.to_string()),
            }
        } else {
            copy_file_into_zip(path, &mut zip)
        };

        match write_result {
            Ok(()) => exported += 1,
            Err(e) => errors.push(format!("{}: {e}", path.display())),
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

fn copy_file_into_zip(path: &Path, zip: &mut ZipWriter<File>) -> Result<(), String> {
    let src = File::open(path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(src);
    copy(&mut reader, zip)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Re-encode still images without EXIF/GPS. Returns `None` for non-image files.
fn processed_image_bytes(
    path: &Path,
    jpeg_quality: u8,
    max_edge: u32,
) -> AppResult<Option<Vec<u8>>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let format = match ext.as_str() {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "png" => ImageFormat::Png,
        "webp" => ImageFormat::WebP,
        "tif" | "tiff" => ImageFormat::Tiff,
        "bmp" => ImageFormat::Bmp,
        _ => return Ok(None),
    };

    let mut img = image::open(path).map_err(|e| AppError::msg(format!("decode failed: {e}")))?;
    if max_edge > 0 {
        let (width, height) = img.dimensions();
        if width.max(height) > max_edge {
            img = img.thumbnail(max_edge, max_edge);
        }
    }
    let mut buf = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            let q = jpeg_quality.clamp(50, 100);
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q);
            encoder
                .encode_image(&img)
                .map_err(|e| AppError::msg(format!("jpeg encode failed: {e}")))?;
        }
        other => {
            img.write_to(&mut buf, other)
                .map_err(|e| AppError::msg(format!("encode failed: {e}")))?;
        }
    }
    Ok(Some(buf.into_inner()))
}

fn unique_entry_name(
    path: &Path,
    captured_at: Option<&str>,
    sequence: usize,
    common_parent: Option<&Path>,
    options: &ExportOptions,
    used_names: &mut HashMap<String, usize>,
) -> String {
    let original = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());
    let base = match options.naming.as_str() {
        "date_filename" => format!(
            "{}_{}",
            captured_at
                .and_then(|date| date.get(..10))
                .unwrap_or("undated")
                .replace(':', "-"),
            original
        ),
        "sequential" => {
            let ext = path
                .extension()
                .map(|s| format!(".{}", s.to_string_lossy()))
                .unwrap_or_default();
            format!("{sequence:04}{ext}")
        }
        _ => original,
    };
    let relative_dir = if options.preserve_folder_structure {
        common_parent
            .and_then(|parent| path.parent().and_then(|dir| dir.strip_prefix(parent).ok()))
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
    } else {
        None
    };

    let key = relative_dir
        .as_ref()
        .map(|dir| format!("{dir}/{base}"))
        .unwrap_or_else(|| base.clone());
    let count = used_names.entry(key).or_insert(0);
    *count += 1;
    let unique_base = if *count == 1 {
        base
    } else {
        let (stem, ext) = match base.rfind('.') {
            Some(index) => (&base[..index], &base[index..]),
            None => (base.as_str(), ""),
        };
        format!("{stem}-{count}{ext}")
    };
    relative_dir
        .map(|dir| format!("{dir}/{unique_base}"))
        .unwrap_or(unique_base)
}

fn common_path(left: &Path, right: &Path) -> PathBuf {
    let components: Vec<_> = left
        .components()
        .zip(right.components())
        .take_while(|(a, b)| a == b)
        .map(|(component, _)| component)
        .collect();
    components
        .iter()
        .fold(PathBuf::new(), |mut path, component| {
            if !matches!(component, Component::CurDir) {
                path.push(component.as_os_str());
            }
            path
        })
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
        let result = export_assets_to_zip(
            &conn,
            &["1".into(), "2".into()],
            &dest,
            ExportOptions::default(),
        )
        .unwrap();

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
            vec!["a/photo.jpg".to_string(), "b/photo.jpg".to_string()]
        );

        let mut buf = Vec::new();
        archive
            .by_name("a/photo.jpg")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert!(!buf.is_empty());
    }

    #[test]
    fn export_rejects_empty_selection() {
        let dir = tempdir().unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        let err = export_assets_to_zip(
            &conn,
            &[],
            &dir.path().join("out.zip"),
            ExportOptions::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("select at least one"));
    }

    #[test]
    fn export_resizes_and_uses_sequential_names() {
        let dir = tempdir().unwrap();
        let photo = dir.path().join("wide.jpg");
        RgbImage::from_pixel(100, 50, Rgb([1, 2, 3]))
            .save(&photo)
            .unwrap();
        let conn = db::open_and_migrate(&dir.path().join("library.db")).unwrap();
        seed_asset(&conn, "wide", &photo);
        let dest = dir.path().join("share.zip");

        export_assets_to_zip(
            &conn,
            &["wide".into()],
            &dest,
            ExportOptions {
                max_edge: 20,
                naming: "sequential".into(),
                preserve_folder_structure: false,
                ..ExportOptions::default()
            },
        )
        .unwrap();

        let mut archive = ZipArchive::new(File::open(&dest).unwrap()).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name("0001.jpg")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert_eq!(image.dimensions(), (20, 10));
    }
}
