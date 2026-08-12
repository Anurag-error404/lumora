use std::path::Path;
use std::time::{Duration, SystemTime};

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::error::AppResult;

/// Keep at most this many days of rotated log files.
const LOG_KEEP_DAYS: u64 = 7;

/// Initialize local-only file + stderr logging under `logs_dir`.
/// Never sends data off-device.
pub fn init_logging(logs_dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(logs_dir)?;
    prune_old_logs(logs_dir, LOG_KEEP_DAYS);
    let file_appender = RollingFileAppender::new(Rotation::DAILY, logs_dir, "photovault.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leak the guard so the worker thread lives for process lifetime.
    std::mem::forget(guard);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(fmt::layer().with_ansi(false).with_writer(non_blocking))
        .try_init();

    tracing::info!(path = %logs_dir.display(), "local logging initialized");
    Ok(())
}

/// Delete rotated log files older than `keep_days`.
pub fn prune_old_logs(logs_dir: &Path, keep_days: u64) {
    if !logs_dir.is_dir() || keep_days == 0 {
        return;
    }
    let Some(cutoff) = SystemTime::now().checked_sub(Duration::from_secs(keep_days * 24 * 60 * 60))
    else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(logs_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("photovault.log") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        if modified < cutoff {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn logging_writes_under_app_data() {
        let dir = tempdir().unwrap();
        let logs = dir.path().join("logs");
        init_logging(&logs).unwrap();
        assert!(logs.exists());
        tracing::info!("test log line");
    }

    #[test]
    fn prune_old_logs_is_safe_on_empty_and_keeps_fresh() {
        let dir = tempdir().unwrap();
        prune_old_logs(dir.path().join("missing").as_path(), 7);
        let fresh = dir.path().join("photovault.log");
        std::fs::write(&fresh, b"ok").unwrap();
        prune_old_logs(dir.path(), 7);
        assert!(fresh.exists());
        // Non-log files are untouched.
        let other = dir.path().join("notes.txt");
        std::fs::write(&other, b"x").unwrap();
        prune_old_logs(dir.path(), 7);
        assert!(other.exists());
    }
}
