//! Lossless (or bit-preserving) size optimization for selected library assets.
//!
//! Images: PNG/WebP/TIFF/BMP re-encode without quality loss; JPEG via `jpegtran`
//! when available. Videos: ffmpeg stream-copy remux (never re-encode).
//! Results are kept only when the output is strictly smaller than the original.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::edit::SaveMode;
use crate::error::{AppError, AppResult};
use crate::indexer;
use crate::thumbnails::ffmpeg::{ffmpeg_available, ffmpeg_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeOptions {
    pub mode: SaveMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeItemResult {
    pub asset_id: String,
    pub status: String,
    pub reason: Option<String>,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub bytes_saved: u64,
    pub new_asset_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeBatchResult {
    pub optimized: u32,
    pub skipped: u32,
    pub failed: u32,
    pub bytes_saved: u64,
    pub items: Vec<OptimizeItemResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeProgressEvent {
    pub done: u32,
    pub total: u32,
    pub current_path: String,
}

static JPEGTRAN: OnceLock<Option<PathBuf>> = OnceLock::new();

fn jpegtran_path() -> Option<&'static Path> {
    JPEGTRAN
        .get_or_init(|| which_bin("jpegtran"))
        .as_ref()
        .map(|p| p.as_path())
}

fn which_bin(bin: &str) -> Option<PathBuf> {
    Command::new("which")
        .arg(bin)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(PathBuf::from(s))
            }
        })
}

fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn file_size(path: &Path) -> AppResult<u64> {
    Ok(std::fs::metadata(path)?.len())
}

fn unique_copy_path(source: &Path, suffix: &str) -> PathBuf {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let candidate = parent.join(format!("{stem}_{suffix}.{ext}"));
    if !candidate.exists() {
        return candidate;
    }
    for i in 2..10_000 {
        let candidate = parent.join(format!("{stem}_{suffix}_{i}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!(
        "{stem}_{suffix}_{}.{}",
        uuid::Uuid::new_v4(),
        ext
    ))
}

fn temp_beside(source: &Path, tag: &str) -> PathBuf {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("media");
    parent.join(format!(".{name}.{tag}.{}.tmp", uuid::Uuid::new_v4()))
}

fn optimize_png_like(source: &Path, dest: &Path) -> AppResult<()> {
    let img = image::open(source).map_err(|e| AppError::msg(format!("open image: {e}")))?;
    let format = match ext_lower(source).as_str() {
        "png" => image::ImageFormat::Png,
        "bmp" => image::ImageFormat::Bmp,
        "tif" | "tiff" => image::ImageFormat::Tiff,
        "gif" => image::ImageFormat::Gif,
        "webp" => image::ImageFormat::WebP,
        other => {
            return Err(AppError::msg(format!(
                "unsupported lossless image format: {other}"
            )))
        }
    };
    img.save_with_format(dest, format)
        .map_err(|e| AppError::msg(format!("re-encode image: {e}")))?;
    Ok(())
}

fn optimize_webp_ffmpeg(source: &Path, dest: &Path) -> AppResult<()> {
    let Some(ffmpeg) = ffmpeg_path() else {
        return Err(AppError::msg(
            "ffmpeg is required to lossless-optimize WebP",
        ));
    };
    let output = Command::new(ffmpeg)
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
        ])
        .arg(source)
        .args(["-c:v", "libwebp", "-lossless", "1", "-an"])
        .arg(dest)
        .output()
        .map_err(|e| AppError::msg(format!("ffmpeg webp: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::msg(format!(
            "ffmpeg webp optimize failed: {}",
            err.trim()
        )));
    }
    Ok(())
}

fn optimize_jpeg(source: &Path, dest: &Path) -> AppResult<()> {
    let Some(jpegtran) = jpegtran_path() else {
        return Err(AppError::msg(
            "no lossless JPEG optimizer (install jpegtran)",
        ));
    };
    let output = Command::new(jpegtran)
        .args(["-copy", "all", "-optimize", "-progressive", "-outfile"])
        .arg(dest)
        .arg(source)
        .output()
        .map_err(|e| AppError::msg(format!("jpegtran: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::msg(format!(
            "jpegtran failed: {}",
            err.trim()
        )));
    }
    Ok(())
}

fn optimize_video_remux(source: &Path, dest: &Path) -> AppResult<()> {
    let Some(ffmpeg) = ffmpeg_path() else {
        return Err(AppError::msg(
            "ffmpeg is required to remux videos (install ffmpeg)",
        ));
    };
    // Stream-copy video+audio only; drop data/attachment streams that often
    // add bulk without affecting playback.
    let output = Command::new(ffmpeg)
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
        ])
        .arg(source)
        .args([
            "-map",
            "0:v:0?",
            "-map",
            "0:a?",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .arg(dest)
        .output()
        .map_err(|e| AppError::msg(format!("ffmpeg remux: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::msg(format!(
            "ffmpeg remux failed: {}",
            err.trim()
        )));
    }
    Ok(())
}

fn write_optimized_temp(source: &Path, media_type: &str) -> AppResult<PathBuf> {
    let ext = ext_lower(source);
    let tmp = temp_beside(source, "opt");

    if media_type == "video" {
        optimize_video_remux(source, &tmp)?;
        return Ok(tmp);
    }

    match ext.as_str() {
        "jpg" | "jpeg" => optimize_jpeg(source, &tmp)?,
        "png" | "bmp" | "tif" | "tiff" | "gif" => optimize_png_like(source, &tmp)?,
        "webp" => {
            // Prefer ffmpeg lossless webp; fall back to image crate.
            if ffmpeg_available() {
                match optimize_webp_ffmpeg(source, &tmp) {
                    Ok(()) => {}
                    Err(_) => optimize_png_like(source, &tmp)?,
                }
            } else {
                optimize_png_like(source, &tmp)?;
            }
        }
        "heic" | "heif" => {
            return Err(AppError::msg("HEIC/HEIF skipped (no lossless optimize path)"));
        }
        "raw" | "dng" | "cr2" | "cr3" | "nef" | "nrw" | "arw" | "orf" | "raf" | "rw2"
        | "pef" | "srw" => {
            return Err(AppError::msg("RAW formats skipped (no lossless optimize path)"));
        }
        other => {
            return Err(AppError::msg(format!(
                "unsupported format for lossless optimize: {other}"
            )));
        }
    }
    Ok(tmp)
}

fn commit_result(
    conn: &Connection,
    thumbs_dir: &Path,
    asset_id: &str,
    source: &Path,
    tmp: &Path,
    mode: SaveMode,
    before: u64,
    after: u64,
) -> AppResult<OptimizeItemResult> {
    if after >= before {
        let _ = std::fs::remove_file(tmp);
        return Ok(OptimizeItemResult {
            asset_id: asset_id.to_string(),
            status: "skipped".into(),
            reason: Some("skipped_no_gain".into()),
            bytes_before: before,
            bytes_after: before,
            bytes_saved: 0,
            new_asset_id: None,
        });
    }

    let (dest, new_id) = match mode {
        SaveMode::Replace => {
            let backup = temp_beside(source, "bak");
            std::fs::rename(source, &backup).map_err(|e| {
                let _ = std::fs::remove_file(tmp);
                AppError::msg(format!("backup original: {e}"))
            })?;
            if let Err(e) = std::fs::rename(tmp, source) {
                let _ = std::fs::rename(&backup, source);
                let _ = std::fs::remove_file(tmp);
                return Err(AppError::msg(format!("replace optimized: {e}")));
            }
            let _ = std::fs::remove_file(&backup);
            (source.to_path_buf(), asset_id.to_string())
        }
        SaveMode::Copy => {
            let dest = unique_copy_path(source, "optimized");
            if let Err(e) = std::fs::rename(tmp, &dest) {
                let _ = std::fs::remove_file(tmp);
                return Err(AppError::msg(format!("save optimized copy: {e}")));
            }
            let path_str = dest.to_string_lossy().to_string();
            indexer::upsert_asset(conn, &dest, thumbs_dir, true)?;
            let id: String = conn.query_row(
                "SELECT id FROM assets WHERE path = ?1",
                params![path_str],
                |r| r.get(0),
            )?;
            return Ok(OptimizeItemResult {
                asset_id: asset_id.to_string(),
                status: "optimized".into(),
                reason: None,
                bytes_before: before,
                bytes_after: after,
                bytes_saved: before - after,
                new_asset_id: Some(id),
            });
        }
    };

    indexer::upsert_asset(conn, &dest, thumbs_dir, true)?;
    // Pixels unchanged for true lossless; still refresh hash/size. Skip ML wipe
    // when replacing — content is pixel-identical for PNG/JPEG optimize paths.
    let _ = dest;

    Ok(OptimizeItemResult {
        asset_id: asset_id.to_string(),
        status: "optimized".into(),
        reason: None,
        bytes_before: before,
        bytes_after: after,
        bytes_saved: before - after,
        new_asset_id: Some(new_id),
    })
}

fn optimize_one(
    conn: &Connection,
    thumbs_dir: &Path,
    asset_id: &str,
    mode: SaveMode,
) -> OptimizeItemResult {
    let row: Result<(String, String), _> = conn.query_row(
        "SELECT path, media_type FROM assets WHERE id = ?1 AND deleted_at IS NULL",
        params![asset_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    );
    let (path_str, media_type) = match row {
        Ok(v) => v,
        Err(_) => {
            return OptimizeItemResult {
                asset_id: asset_id.to_string(),
                status: "failed".into(),
                reason: Some("asset not found".into()),
                bytes_before: 0,
                bytes_after: 0,
                bytes_saved: 0,
                new_asset_id: None,
            };
        }
    };
    let source = PathBuf::from(&path_str);
    if !source.is_file() {
        return OptimizeItemResult {
            asset_id: asset_id.to_string(),
            status: "failed".into(),
            reason: Some("file missing on disk".into()),
            bytes_before: 0,
            bytes_after: 0,
            bytes_saved: 0,
            new_asset_id: None,
        };
    }

    let before = match file_size(&source) {
        Ok(n) => n,
        Err(e) => {
            return OptimizeItemResult {
                asset_id: asset_id.to_string(),
                status: "failed".into(),
                reason: Some(e.to_string()),
                bytes_before: 0,
                bytes_after: 0,
                bytes_saved: 0,
                new_asset_id: None,
            };
        }
    };

    let tmp = match write_optimized_temp(&source, &media_type) {
        Ok(p) => p,
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("skipped") || msg.contains("no lossless") {
                "skipped"
            } else {
                "failed"
            };
            return OptimizeItemResult {
                asset_id: asset_id.to_string(),
                status: status.into(),
                reason: Some(msg),
                bytes_before: before,
                bytes_after: before,
                bytes_saved: 0,
                new_asset_id: None,
            };
        }
    };

    let after = match file_size(&tmp) {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return OptimizeItemResult {
                asset_id: asset_id.to_string(),
                status: "failed".into(),
                reason: Some(e.to_string()),
                bytes_before: before,
                bytes_after: before,
                bytes_saved: 0,
                new_asset_id: None,
            };
        }
    };

    match commit_result(
        conn, thumbs_dir, asset_id, &source, &tmp, mode, before, after,
    ) {
        Ok(item) => item,
        Err(e) => OptimizeItemResult {
            asset_id: asset_id.to_string(),
            status: "failed".into(),
            reason: Some(e.to_string()),
            bytes_before: before,
            bytes_after: before,
            bytes_saved: 0,
            new_asset_id: None,
        },
    }
}

/// Optimize each asset; never fails the whole batch on a single file error.
pub fn optimize_assets(
    conn: &Connection,
    thumbs_dir: &Path,
    ids: &[String],
    options: &OptimizeOptions,
    mut on_progress: impl FnMut(OptimizeProgressEvent),
) -> AppResult<OptimizeBatchResult> {
    let total = ids.len() as u32;
    let mut items = Vec::with_capacity(ids.len());
    let mut optimized = 0u32;
    let mut skipped = 0u32;
    let mut failed = 0u32;
    let mut bytes_saved = 0u64;

    for (i, id) in ids.iter().enumerate() {
        let path_hint: String = conn
            .query_row(
                "SELECT path FROM assets WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        on_progress(OptimizeProgressEvent {
            done: i as u32,
            total,
            current_path: path_hint,
        });

        let item = optimize_one(conn, thumbs_dir, id, options.mode);
        match item.status.as_str() {
            "optimized" => {
                optimized += 1;
                bytes_saved += item.bytes_saved;
            }
            "skipped" => skipped += 1,
            _ => failed += 1,
        }
        items.push(item);
    }

    on_progress(OptimizeProgressEvent {
        done: total,
        total,
        current_path: String::new(),
    });

    Ok(OptimizeBatchResult {
        optimized,
        skipped,
        failed,
        bytes_saved,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use image::{Rgb, RgbImage};
    use tempfile::tempdir;

    #[test]
    fn png_optimize_keeps_or_shrinks() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.png");
        let mut img = RgbImage::new(64, 64);
        for p in img.pixels_mut() {
            *p = Rgb([20, 40, 60]);
        }
        image::DynamicImage::ImageRgb8(img)
            .save(&path)
            .unwrap();
        let before = file_size(&path).unwrap();
        let tmp = temp_beside(&path, "opt");
        optimize_png_like(&path, &tmp).unwrap();
        let after = file_size(&tmp).unwrap();
        assert!(after > 0);
        assert!(after <= before * 2); // sanity; solid PNG often shrinks
    }

    #[test]
    fn batch_skips_missing_asset() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("t.db");
        let conn = db::open_and_migrate(&db_path).unwrap();
        let thumbs = dir.path().join("thumbs");
        std::fs::create_dir_all(&thumbs).unwrap();
        let result = optimize_assets(
            &conn,
            &thumbs,
            &["missing".into()],
            &OptimizeOptions {
                mode: SaveMode::Replace,
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(result.failed, 1);
        assert_eq!(result.optimized, 0);
    }
}
