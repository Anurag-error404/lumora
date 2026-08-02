//! Video trim + crop via system ffmpeg.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::edit::{CropRect, EditResult, SaveMode};
use crate::error::{AppError, AppResult};
use crate::indexer;
use crate::ml::catalog::ModelKind;
use crate::search;
use crate::semantic;
use crate::thumbnails::ffmpeg::{ffmpeg_path, ffprobe_path, probe_video};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoEditOps {
    /// Inclusive trim start in seconds.
    pub trim_start: f64,
    /// Exclusive trim end in seconds. Must be > trim_start.
    pub trim_end: f64,
    /// Optional normalized crop (0…1) in the source frame.
    pub crop: Option<CropRect>,
    /// When true (or when crop is set), re-encode for frame-accurate trim.
    /// When false and no crop, prefer stream-copy (may snap to keyframes).
    #[serde(default)]
    pub accurate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoProbeInfo {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub ffmpeg_available: bool,
}

pub fn probe_asset(conn: &Connection, asset_id: &str) -> AppResult<VideoProbeInfo> {
    let (path_str, media_type): (String, String) = conn
        .query_row(
            "SELECT path, media_type FROM assets WHERE id = ?1 AND deleted_at IS NULL",
            params![asset_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| AppError::msg("asset not found"))?;
    if media_type != "video" {
        return Err(AppError::msg("only videos can be probed here"));
    }
    let path = PathBuf::from(&path_str);
    let probe = probe_video(&path);
    Ok(VideoProbeInfo {
        width: probe.width,
        height: probe.height,
        duration_ms: probe.duration_ms,
        ffmpeg_available: ffmpeg_path().is_some(),
    })
}

fn validate_ops(ops: &VideoEditOps, duration_s: Option<f64>) -> AppResult<()> {
    if !ops.trim_start.is_finite() || !ops.trim_end.is_finite() {
        return Err(AppError::msg("trim times must be finite"));
    }
    if ops.trim_start < 0.0 {
        return Err(AppError::msg("trim start must be >= 0"));
    }
    if ops.trim_end <= ops.trim_start {
        return Err(AppError::msg("trim end must be after trim start"));
    }
    if let Some(dur) = duration_s {
        if ops.trim_start >= dur {
            return Err(AppError::msg("trim start is past the end of the video"));
        }
    }
    if let Some(crop) = &ops.crop {
        if crop.width <= 0.0 || crop.height <= 0.0 {
            return Err(AppError::msg("crop size must be positive"));
        }
        if crop.x < 0.0 || crop.y < 0.0 || crop.x + crop.width > 1.001 || crop.y + crop.height > 1.001
        {
            return Err(AppError::msg("crop must stay within the frame"));
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
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
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
    parent.join(format!("{stem}_edited_{}.{}", uuid::Uuid::new_v4(), ext))
}

fn crop_filter(crop: &CropRect, width: i64, height: i64) -> String {
    let w = ((crop.width.clamp(0.0, 1.0) * width as f32).round() as i64).max(2);
    let h = ((crop.height.clamp(0.0, 1.0) * height as f32).round() as i64).max(2);
    // libx264 needs even dimensions.
    let w = w - (w % 2);
    let h = h - (h % 2);
    let x = ((crop.x.clamp(0.0, 1.0) * width as f32).round() as i64).max(0);
    let y = ((crop.y.clamp(0.0, 1.0) * height as f32).round() as i64).max(0);
    let x = x.min((width - w).max(0));
    let y = y.min((height - h).max(0));
    format!("crop={w}:{h}:{x}:{y}")
}

fn run_ffmpeg(args: &[String]) -> AppResult<()> {
    let ffmpeg = ffmpeg_path().ok_or_else(|| {
        AppError::msg(
            "ffmpeg not found on PATH — install it (e.g. brew install ffmpeg) to edit videos",
        )
    })?;
    let output = Command::new(ffmpeg)
        .args(args)
        .output()
        .map_err(|e| AppError::msg(format!("ffmpeg: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("ffmpeg failed");
        return Err(AppError::msg(format!("ffmpeg video edit failed: {last}")));
    }
    Ok(())
}

fn encode_video(source: &Path, dest: &Path, ops: &VideoEditOps, width: i64, height: i64) -> AppResult<()> {
    let needs_filter = ops.crop.is_some();
    let reencode = ops.accurate || needs_filter;

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
    ];

    if !reencode {
        // Input seeking for faster stream-copy (keyframe-aligned).
        args.push("-ss".into());
        args.push(format!("{:.3}", ops.trim_start));
        args.push("-to".into());
        args.push(format!("{:.3}", ops.trim_end));
        args.push("-i".into());
        args.push(source.to_string_lossy().into_owned());
        args.extend([
            "-map".into(),
            "0:v:0?".into(),
            "-map".into(),
            "0:a?".into(),
            "-c".into(),
            "copy".into(),
            "-avoid_negative_ts".into(),
            "make_zero".into(),
        ]);
    } else {
        args.push("-i".into());
        args.push(source.to_string_lossy().into_owned());
        args.push("-ss".into());
        args.push(format!("{:.3}", ops.trim_start));
        args.push("-to".into());
        args.push(format!("{:.3}", ops.trim_end));
        if let Some(crop) = &ops.crop {
            args.push("-vf".into());
            args.push(crop_filter(crop, width, height));
        }
        args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "fast".into(),
            "-crf".into(),
            "18".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "192k".into(),
            "-movflags".into(),
            "+faststart".into(),
        ]);
    }

    args.push(dest.to_string_lossy().into_owned());
    run_ffmpeg(&args)
}

fn invalidate_derived(conn: &Connection, thumbs_dir: &Path, asset_id: &str) -> AppResult<bool> {
    let removed = conn.execute(
        "DELETE FROM asset_embeddings WHERE asset_id = ?1 AND model_id = ?2",
        params![asset_id, semantic::IMAGE_MODEL_ID],
    )?;
    conn.execute(
        "DELETE FROM ml_jobs WHERE asset_id = ?1 AND kind = ?2",
        params![asset_id, ModelKind::ClipImage.as_str()],
    )?;
    if removed > 0 {
        semantic::ann::mark_dirty();
    }
    let _ = crate::ocr::invalidate_asset(conn, asset_id);
    if let Some(app_data) = thumbs_dir.parent() {
        let faces_dir = app_data.join("faces");
        let _ = crate::faces::invalidate_asset(conn, &faces_dir, asset_id);
    }
    let _ = crate::tags::invalidate_asset(conn, asset_id);
    let _ = crate::captions::invalidate_asset(conn, asset_id);
    Ok(removed > 0)
}

fn load_asset(conn: &Connection, id: &str) -> AppResult<crate::models::AssetSummary> {
    conn.query_row(
        "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                created_at, captured_at, indexed_at, favorite, rating, color_label,
                thumbnail_path, camera, lens, deleted_at
         FROM assets WHERE id = ?1",
        params![id],
        search::map_asset,
    )
    .map_err(|_| AppError::msg("edited video not found after save"))
}

/// Trim/crop a video and replace or save a sibling copy.
pub fn apply_video_edit(
    conn: &Connection,
    thumbs_dir: &Path,
    asset_id: &str,
    ops: &VideoEditOps,
    mode: SaveMode,
) -> AppResult<EditResult> {
    let _ = ffprobe_path(); // warm cache
    let (path_str, media_type): (String, String) = conn.query_row(
        "SELECT path, media_type FROM assets WHERE id = ?1 AND deleted_at IS NULL",
        params![asset_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if media_type != "video" {
        return Err(AppError::msg("only videos can be edited here"));
    }
    let source = PathBuf::from(&path_str);
    if !source.is_file() {
        return Err(AppError::msg("video file is missing on disk"));
    }

    let probe = probe_video(&source);
    let duration_s = probe.duration_ms.map(|ms| ms as f64 / 1000.0);
    validate_ops(ops, duration_s)?;

    let width = probe.width.unwrap_or(0).max(2);
    let height = probe.height.unwrap_or(0).max(2);

    let tmp = source.with_extension("lumora-vedit-tmp.mp4");
    if let Err(e) = encode_video(&source, &tmp, ops, width, height) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if !tmp.is_file() {
        return Err(AppError::msg("ffmpeg produced no output file"));
    }

    let dest = match mode {
        SaveMode::Replace => {
            let backup = source.with_extension("lumora-vedit-bak");
            std::fs::rename(&source, &backup).map_err(|e| {
                let _ = std::fs::remove_file(&tmp);
                AppError::msg(format!("backup original: {e}"))
            })?;
            if let Err(e) = std::fs::rename(&tmp, &source) {
                let _ = std::fs::rename(&backup, &source);
                let _ = std::fs::remove_file(&tmp);
                return Err(AppError::msg(format!("replace video: {e}")));
            }
            let _ = std::fs::remove_file(&backup);
            source.clone()
        }
        SaveMode::Copy => {
            let dest = unique_copy_path(&source);
            // Prefer keeping original extension when container matches.
            let dest = if source.extension().and_then(|e| e.to_str()) == Some("mp4") {
                dest
            } else {
                dest.with_extension("mp4")
            };
            if let Err(e) = std::fs::rename(&tmp, &dest) {
                let _ = std::fs::remove_file(&tmp);
                return Err(AppError::msg(format!("save video copy: {e}")));
            }
            dest
        }
    };

    indexer::upsert_asset(conn, &dest, thumbs_dir, true)?;
    let id = match mode {
        SaveMode::Replace => asset_id.to_string(),
        SaveMode::Copy => {
            let path_str = dest.to_string_lossy().to_string();
            conn.query_row(
                "SELECT id FROM assets WHERE path = ?1",
                params![path_str],
                |r| r.get(0),
            )?
        }
    };

    let embedding_queued = invalidate_derived(conn, thumbs_dir, &id)?;
    let asset = load_asset(conn, &id)?;
    Ok(EditResult {
        asset,
        mode,
        embedding_queued,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_inverted_trim() {
        let err = validate_ops(
            &VideoEditOps {
                trim_start: 5.0,
                trim_end: 2.0,
                crop: None,
                accurate: false,
            },
            Some(10.0),
        )
        .unwrap_err();
        assert!(err.to_string().contains("trim end"));
    }

    #[test]
    fn crop_filter_even_dims() {
        let f = crop_filter(
            &CropRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            1920,
            1080,
        );
        assert_eq!(f, "crop=1920:1080:0:0");
    }
}
