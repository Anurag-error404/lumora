//! Runtime helpers that apply user preferences to background work.
//!
//! Workers call [`background_allowed`] before draining a batch, and
//! [`throttle`] for sleep intervals that scale with the CPU profile.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::preferences::{PerformancePrefs, Preferences};

/// Seconds of UI inactivity before `"idle"` background mode will run jobs.
pub const IDLE_THRESHOLD_SECS: u64 = 60;

/// Last user activity (unix seconds). Updated from the UI via `ping_user_activity`.
static LAST_USER_ACTIVITY_SECS: AtomicU64 = AtomicU64::new(0);

pub fn touch_user_activity() {
    LAST_USER_ACTIVITY_SECS.store(now_secs(), Ordering::Relaxed);
}

pub fn last_user_activity_secs() -> u64 {
    LAST_USER_ACTIVITY_SECS.load(Ordering::Relaxed)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Whether AI / Places background workers should process a batch right now.
pub fn background_allowed(prefs: &Preferences) -> bool {
    match prefs.ai.background_processing.as_str() {
        "paused" => false,
        "idle" => {
            let last = last_user_activity_secs();
            // No ping yet (cold start) → treat as idle so work can begin.
            last == 0 || now_secs().saturating_sub(last) >= IDLE_THRESHOLD_SECS
        }
        _ => true, // "always" and unknown
    }
}

/// Pause ML when on battery and the preference is on.
pub fn battery_blocks(prefs: &Preferences) -> bool {
    prefs.performance.pause_on_battery && on_battery()
}

pub fn should_run_background(prefs: &Preferences) -> bool {
    background_allowed(prefs) && !battery_blocks(prefs)
}

#[derive(Debug, Clone, Copy)]
pub struct Throttle {
    /// Sleep between successful batches (ms).
    pub between_ms: u64,
    /// Sleep when idle / waiting (ms).
    pub idle_ms: u64,
    /// Suggested ONNX intra-op threads.
    pub intra_threads: usize,
    /// Suggested batch size multiplier vs the worker default (1 = default).
    #[allow(dead_code)]
    pub batch_scale: f32,
}

pub fn throttle(perf: &PerformancePrefs) -> Throttle {
    match perf.cpu_profile.as_str() {
        "eco" => Throttle {
            between_ms: 40,
            idle_ms: 1500,
            intra_threads: 1,
            batch_scale: 0.5,
        },
        "aggressive" => Throttle {
            between_ms: 1,
            idle_ms: 250,
            intra_threads: 0, // 0 = let ORT pick
            batch_scale: 1.5,
        },
        _ => Throttle {
            // balanced
            between_ms: 8,
            idle_ms: 750,
            intra_threads: 2,
            batch_scale: 1.0,
        },
    }
}

/// Resolve intra-op threads from device preference + CPU profile.
pub fn ort_intra_threads(prefs: &Preferences) -> usize {
    let base = throttle(&prefs.performance).intra_threads;
    match prefs.ai.processing_device.as_str() {
        "cpu" => base.max(1).min(2),
        "gpu" => {
            // No dedicated GPU EP wired yet — use more CPU threads as a stand-in
            // so the control still affects throughput on every platform.
            if base == 0 {
                4
            } else {
                base.saturating_mul(2).max(4)
            }
        }
        _ => base, // automatic
    }
}

/// True when the machine is on battery power (best-effort; false if unknown).
pub fn on_battery() -> bool {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("pmset")
            .args(["-g", "batt"])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            return text.contains("Battery Power");
        }
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let typ = std::fs::read_to_string(path.join("type")).unwrap_or_default();
            if !typ.trim().eq_ignore_ascii_case("Battery") {
                continue;
            }
            let status = std::fs::read_to_string(path.join("status")).unwrap_or_default();
            if status.trim().eq_ignore_ascii_case("Discharging") {
                return true;
            }
        }
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        // Best-effort via PowerShell; fail open (do not pause) on error.
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Battery).BatteryStatus",
            ])
            .output();
        if let Ok(out) = output {
            let text = String::from_utf8_lossy(&out.stdout);
            // 1 = discharging on BatteryStatus enum
            return text.lines().any(|l| l.trim() == "1");
        }
        return false;
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// Match a path against user ignore patterns.
///
/// Patterns:
/// - `*.ext` — extension match
/// - `name` or `prefix*` — file-name glob (only `*` wildcard)
/// - `/segment/` or `segment` — path component / substring match (case-insensitive)
pub fn path_is_ignored(path: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let full = path.to_string_lossy().replace('\\', "/");
    let full_lower = full.to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    for raw in patterns {
        let pat = raw.trim();
        if pat.is_empty() {
            continue;
        }
        let pat_lower = pat.to_ascii_lowercase();
        if pat_lower.starts_with("*.") {
            let ext = &pat_lower[2..];
            if name.ends_with(&format!(".{ext}")) || name.ends_with(ext) && name.contains('.') {
                let file_ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase());
                if file_ext.as_deref() == Some(ext) {
                    return true;
                }
            }
            continue;
        }
        if pat_lower.contains('*') {
            if glob_match(&pat_lower, &name) || glob_match(&pat_lower, &full_lower) {
                return true;
            }
            continue;
        }
        if full_lower.contains(&pat_lower) || name == pat_lower {
            return true;
        }
    }
    false
}

fn glob_match(pattern: &str, text: &str) -> bool {
    // Single-segment `*` glob.
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !rest.starts_with(part) {
                return false;
            }
            rest = &rest[part.len()..];
        } else if i == parts.len() - 1 {
            if !rest.ends_with(part) {
                return false;
            }
        } else if let Some(idx) = rest.find(part) {
            rest = &rest[idx + part.len()..];
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn ignore_extension_and_substring() {
        let path = PathBuf::from("/Photos/trip/IMG_001.TMP");
        assert!(path_is_ignored(&path, &["*.tmp".into()]));
        assert!(path_is_ignored(&path, &["/trip/".into()]));
        assert!(!path_is_ignored(&path, &["*.raw".into()]));
    }

    #[test]
    fn ignore_glob_filename() {
        let path = PathBuf::from("/a/b/secret-cache.jpg");
        assert!(path_is_ignored(&path, &["*cache*".into()]));
        assert!(!path_is_ignored(&path, &["thumb*".into()]));
    }

    #[test]
    fn background_idle_requires_inactivity() {
        let mut prefs = Preferences::default();
        prefs.ai.background_processing = "idle".into();
        touch_user_activity();
        assert!(!background_allowed(&prefs));
        // Simulate stale activity
        LAST_USER_ACTIVITY_SECS.store(
            now_secs().saturating_sub(IDLE_THRESHOLD_SECS + 5),
            Ordering::Relaxed,
        );
        assert!(background_allowed(&prefs));
    }
}
