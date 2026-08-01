//! Decode Apple HEIC/HEIF stills via system tools.
//!
//! The `image` crate has no HEIC decoder. Phone libraries (Samsung/iPhone) are
//! often mostly HEIC, so we convert with **ffmpeg** (preferred — typically ~3×
//! faster than sips on Apple Silicon) or macOS `sips` as fallback.
//! Conversions always request a max edge so we never materialize a 12MP buffer
//! just to build a 320px library thumb.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use image::DynamicImage;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

use super::ffmpeg;
use super::open_oriented_native;

/// Cap used when callers need a decoded bitmap (import / AI), not a final thumb.
/// Still far smaller than phone originals (often 12MP+).
pub const HEIC_WORKING_MAX_EDGE: u32 = 1536;

pub fn is_heic_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            e == "heic" || e == "heif" || e == "hif"
        })
        .unwrap_or(false)
}

/// Cheap width/height without decoding pixels (sips metadata or ffprobe).
pub fn probe_dimensions(path: &Path) -> Option<(u32, u32)> {
    #[cfg(target_os = "macos")]
    {
        if let Some(dims) = probe_dimensions_sips(path) {
            return Some(dims);
        }
    }
    probe_dimensions_ffprobe(path)
}

/// Decode HEIC/HEIF to an oriented image, capped at [`HEIC_WORKING_MAX_EDGE`].
pub fn open_heic(path: &Path) -> AppResult<DynamicImage> {
    open_heic_scaled(path, HEIC_WORKING_MAX_EDGE)
}

/// Decode HEIC/HEIF scaled so the longest edge is at most `max_edge`.
pub fn open_heic_scaled(path: &Path, max_edge: u32) -> AppResult<DynamicImage> {
    let tmp = temp_jpeg_path()?;
    match convert_heic_jpeg(path, &tmp, max_edge) {
        Ok(()) => match open_oriented_native(&tmp) {
            Ok(img) => {
                let _ = std::fs::remove_file(&tmp);
                Ok(img)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(AppError::msg(format!(
                    "HEIC convert produced unreadable JPEG for {}: {e}",
                    path.display()
                )))
            }
        },
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(AppError::msg(format!(
                "HEIC decode failed for {}: {e}",
                path.display()
            )))
        }
    }
}

/// Write a JPEG thumbnail directly from HEIC — no full-res decode in-process.
pub fn write_thumbnail(source: &Path, dest: &Path, max_edge: u32) -> AppResult<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    convert_heic_jpeg(source, dest, max_edge).map_err(AppError::msg)?;
    if !dest.is_file() {
        return Err(AppError::msg(format!(
            "HEIC thumbnail missing after convert: {}",
            dest.display()
        )));
    }
    Ok(())
}

fn convert_heic_jpeg(source: &Path, dest: &Path, max_edge: u32) -> Result<(), String> {
    let mut last_err = String::from("no HEIC decoder available");

    // ffmpeg is typically much faster than sips for HEIC→JPEG+scale on macOS.
    if let Some(ffmpeg) = ffmpeg::ffmpeg_path() {
        match convert_with_ffmpeg(ffmpeg, source, dest, max_edge) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
        let _ = std::fs::remove_file(dest);
    }

    #[cfg(target_os = "macos")]
    {
        match convert_with_sips(source, dest, max_edge) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
        let _ = std::fs::remove_file(dest);
    }

    Err(last_err)
}

fn temp_jpeg_path() -> AppResult<PathBuf> {
    let dir = std::env::temp_dir().join("lumora-heic");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{}.jpg", Uuid::new_v4())))
}

fn convert_with_ffmpeg(
    ffmpeg: &Path,
    source: &Path,
    dest: &Path,
    max_edge: u32,
) -> Result<(), String> {
    let scale = format!("scale='min({max_edge},iw)':-2");
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
        ])
        .arg(source)
        .args(["-frames:v", "1", "-vf", &scale, "-q:v", "5"])
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() && dest.is_file() {
        return Ok(());
    }
    let _ = std::fs::remove_file(dest);
    let err = String::from_utf8_lossy(&output.stderr);
    let last = err
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("ffmpeg HEIC convert failed");
    Err(last.to_string())
}

#[cfg(target_os = "macos")]
fn convert_with_sips(source: &Path, dest: &Path, max_edge: u32) -> Result<(), String> {
    // `-Z` fits the image inside max_edge×max_edge (aspect preserved) before
    // JPEG encode — much faster than decoding a full phone-resolution HEIC.
    let edge = max_edge.max(1).to_string();
    let output = Command::new("sips")
        .args(["-Z", &edge, "-s", "format", "jpeg"])
        .arg(source)
        .arg("--out")
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() && dest.is_file() {
        return Ok(());
    }
    let _ = std::fs::remove_file(dest);
    let err = String::from_utf8_lossy(&output.stderr);
    let last = err
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("sips HEIC convert failed");
    Err(last.to_string())
}

#[cfg(target_os = "macos")]
fn probe_dimensions_sips(path: &Path) -> Option<(u32, u32)> {
    let output = Command::new("sips")
        .args(["-g", "pixelWidth", "-g", "pixelHeight"])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut width = None;
    let mut height = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("pixelWidth:") {
            width = rest.trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix("pixelHeight:") {
            height = rest.trim().parse().ok();
        }
    }
    match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Some((w, h)),
        _ => None,
    }
}

fn probe_dimensions_ffprobe(path: &Path) -> Option<(u32, u32)> {
    let ffprobe = ffmpeg::ffprobe_path()?;
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text
        .split(['x', ',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let w: u32 = parts.next()?.parse().ok()?;
    let h: u32 = parts.next()?.parse().ok()?;
    if w > 0 && h > 0 {
        Some((w, h))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_heic_extensions() {
        assert!(is_heic_path(Path::new("/a/b/IMG_001.HEIC")));
        assert!(is_heic_path(Path::new("/a/b/img.heif")));
        assert!(!is_heic_path(Path::new("/a/b/img.jpg")));
    }

    #[test]
    fn thumb_max_edge_is_positive() {
        assert!(super::super::THUMB_MAX_EDGE >= 64);
        assert!(HEIC_WORKING_MAX_EDGE >= super::super::THUMB_MAX_EDGE);
    }
}
