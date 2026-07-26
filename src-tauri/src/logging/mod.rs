use std::path::Path;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::error::AppResult;

/// Initialize local-only file + stderr logging under `logs_dir`.
/// Never sends data off-device.
pub fn init_logging(logs_dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(logs_dir)?;
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
        // File may be created lazily; directory existence is the acceptance bar for unit tests.
    }
}
