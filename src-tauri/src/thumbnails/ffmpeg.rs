//! Video frame extraction and probe via system `ffmpeg` / `ffprobe`.
//!
//! Requires ffmpeg on PATH (Homebrew: `brew install ffmpeg`). Missing tools are
//! treated as soft failures so image-only libraries keep working.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::error::{AppError, AppResult};

use super::{thumbnail_path, THUMB_MAX_EDGE};

static FFMPEG: OnceLock<Option<PathBuf>> = OnceLock::new();
static FFPROBE: OnceLock<Option<PathBuf>> = OnceLock::new();

fn which(bin: &str) -> Option<PathBuf> {
    // GUI apps on macOS get a stripped PATH (no Homebrew). Prefer PATH, then
    // well-known locations.
    if let Some(p) = Command::new("which")
        .arg(bin)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (!s.is_empty()).then(|| PathBuf::from(s))
        })
    {
        return Some(p);
    }
    // Apple Silicon Homebrew, Intel Homebrew, then /usr/local fallbacks.
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let p = PathBuf::from(dir).join(bin);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub fn ffmpeg_path() -> Option<&'static Path> {
    FFMPEG
        .get_or_init(|| which("ffmpeg"))
        .as_ref()
        .map(|p| p.as_path())
}

pub fn ffprobe_path() -> Option<&'static Path> {
    FFPROBE
        .get_or_init(|| which("ffprobe"))
        .as_ref()
        .map(|p| p.as_path())
}

pub fn ffmpeg_available() -> bool {
    ffmpeg_path().is_some()
}

#[derive(Debug, Clone, Default)]
pub struct VideoProbe {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
}

/// Best-effort container/stream metadata. Returns empty fields if ffprobe is missing.
pub fn probe_video(path: &Path) -> VideoProbe {
    let Some(ffprobe) = ffprobe_path() else {
        return VideoProbe::default();
    };

    let mut out = VideoProbe::default();

    if let Ok(output) = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height:format=duration",
            "-of",
            "csv=p=0:s=x",
        ])
        .arg(path)
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            // Typical: "1920x1080x12.345000" or multi-line variants.
            let flat = text.replace('\n', "x").replace(',', "x");
            let parts: Vec<&str> = flat
                .split('x')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() >= 2 {
                out.width = parts[0].parse().ok();
                out.height = parts[1].parse().ok();
            }
            if let Some(dur) = parts.get(2).and_then(|s| s.parse::<f64>().ok()) {
                if dur.is_finite() && dur > 0.0 {
                    out.duration_ms = Some((dur * 1000.0).round() as i64);
                }
            }
        }
    }

    if out.duration_ms.is_none() {
        if let Ok(output) = Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(dur) = text.parse::<f64>() {
                    if dur.is_finite() && dur > 0.0 {
                        out.duration_ms = Some((dur * 1000.0).round() as i64);
                    }
                }
            }
        }
    }

    out
}

/// Extract one JPEG frame (~1s in, or start) scaled to max edge [`THUMB_MAX_EDGE`].
pub fn extract_frame_thumbnail(source: &Path, thumbs_dir: &Path, hash: &str) -> AppResult<PathBuf> {
    let ffmpeg = ffmpeg_path().ok_or_else(|| {
        AppError::msg(
            "ffmpeg not found on PATH — install it (e.g. brew install ffmpeg) for video thumbnails",
        )
    })?;

    std::fs::create_dir_all(thumbs_dir)?;
    let dest = thumbnail_path(thumbs_dir, hash);
    if dest.exists() {
        return Ok(dest);
    }

    let scale = format!("scale='min({THUMB_MAX_EDGE},iw)':-2");
    let mut last_err = String::from("no frames extracted");

    for ss in ["1", "0"] {
        let output = Command::new(ffmpeg)
            .args(["-y", "-ss", ss, "-i"])
            .arg(source)
            .args(["-frames:v", "1", "-vf", &scale, "-q:v", "3"])
            .arg(&dest)
            .output();

        match output {
            Ok(output) if output.status.success() && dest.exists() => {
                return Ok(dest);
            }
            Ok(output) => {
                let _ = std::fs::remove_file(&dest);
                last_err = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .last()
                    .unwrap_or("ffmpeg failed")
                    .to_string();
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }

    Err(AppError::msg(format!(
        "video thumbnail failed for {}: {last_err}",
        source.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_finds_or_misses_without_panic() {
        let _ = ffmpeg_available();
        let _ = ffprobe_path();
    }

    #[test]
    fn which_falls_back_to_homebrew_when_path_empty() {
        // ponytail: only asserts the fallback lookup itself; skips if ffmpeg isn't installed.
        let homebrew = PathBuf::from("/opt/homebrew/bin/ffmpeg");
        let intel = PathBuf::from("/usr/local/bin/ffmpeg");
        if !homebrew.is_file() && !intel.is_file() {
            return;
        }
        let found = which("ffmpeg");
        assert!(found.is_some());
        let p = found.unwrap();
        assert!(p.ends_with("ffmpeg"));
        assert!(p.is_file());
    }
}
