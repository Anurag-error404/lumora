//! Decode camera RAW (DNG, CR2, …) via system tools.
//!
//! The `image` crate cannot decode most RAW containers — Samsung/Adobe DNG often
//! reports as TIFF with proprietary compression (`Tiff is not supported`). On
//! macOS, `sips` reads these correctly; ffmpeg only works for some formats, so
//! we prefer sips first (opposite of the HEIC path).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use image::DynamicImage;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

use super::ffmpeg;
use super::open_oriented_native;

/// Keep in sync with [`crate::indexer::scan::RAW_EXT`].
const RAW_EXT: &[&str] = &[
    "raw", "dng", "cr2", "cr3", "nef", "nrw", "arw", "srf", "sr2", "orf", "raf", "rw2", "pef",
    "srw", "x3f", "3fr", "erf", "mrw", "dcr", "kdc",
];

/// Cap when callers need a decoded bitmap (import / AI), not a final thumb.
pub const RAW_WORKING_MAX_EDGE: u32 = 1536;

pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| RAW_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Decode RAW scaled so the longest edge is at most `max_edge`.
pub fn open_raw_scaled(path: &Path, max_edge: u32) -> AppResult<DynamicImage> {
    let tmp = temp_jpeg_path()?;
    match convert_raw_jpeg(path, &tmp, max_edge) {
        Ok(()) => match open_oriented_native(&tmp) {
            Ok(img) => {
                let _ = std::fs::remove_file(&tmp);
                Ok(img)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(AppError::msg(format!(
                    "RAW convert produced unreadable JPEG for {}: {e}",
                    path.display()
                )))
            }
        },
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(AppError::msg(format!(
                "RAW decode failed for {}: {e}",
                path.display()
            )))
        }
    }
}

pub fn open_raw(path: &Path) -> AppResult<DynamicImage> {
    open_raw_scaled(path, RAW_WORKING_MAX_EDGE)
}

/// Write a JPEG thumbnail directly from RAW — no full-res decode in-process.
pub fn write_thumbnail(source: &Path, dest: &Path, max_edge: u32) -> AppResult<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    convert_raw_jpeg(source, dest, max_edge).map_err(AppError::msg)?;
    if !dest.is_file() {
        return Err(AppError::msg(format!(
            "RAW thumbnail missing after convert: {}",
            dest.display()
        )));
    }
    Ok(())
}

fn convert_raw_jpeg(source: &Path, dest: &Path, max_edge: u32) -> Result<(), String> {
    let mut last_err = String::from("no RAW decoder available");

    // sips handles Samsung/Adobe DNG that ffmpeg rejects.
    #[cfg(target_os = "macos")]
    {
        match convert_with_sips(source, dest, max_edge) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
        let _ = std::fs::remove_file(dest);
    }

    if let Some(ffmpeg) = ffmpeg::ffmpeg_path() {
        match convert_with_ffmpeg(ffmpeg, source, dest, max_edge) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
        let _ = std::fs::remove_file(dest);
    }

    Err(last_err)
}

fn temp_jpeg_path() -> AppResult<PathBuf> {
    let dir = std::env::temp_dir().join("lumora-raw");
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
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
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
        .unwrap_or("ffmpeg RAW convert failed");
    Err(last.to_string())
}

#[cfg(target_os = "macos")]
fn convert_with_sips(source: &Path, dest: &Path, max_edge: u32) -> Result<(), String> {
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
        .unwrap_or("sips RAW convert failed");
    Err(last.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_raw_extensions() {
        assert!(is_raw_path(Path::new("/a/shot.DNG")));
        assert!(is_raw_path(Path::new("/a/shot.cr3")));
        assert!(!is_raw_path(Path::new("/a/shot.jpg")));
        assert!(!is_raw_path(Path::new("/a/shot.heic")));
    }
}
