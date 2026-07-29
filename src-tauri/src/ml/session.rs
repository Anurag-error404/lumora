//! Shared ONNX session construction respecting AI / performance preferences.

use std::path::Path;

use ort::session::Session;

use crate::error::{AppError, AppResult};
use crate::preferences;
use crate::prefs_runtime;

pub fn load_session(path: &Path, label: &str) -> AppResult<Session> {
    let mut builder = Session::builder()
        .map_err(|e| AppError::msg(format!("ort session builder ({label}): {e}")))?;

    // Best-effort: honour processing_device + cpu_profile via intra-op threads.
    // Dedicated GPU/CoreML EPs are not wired yet; "gpu" maps to more CPU threads.
    let prefs = preferences::load_current();
    let threads = prefs_runtime::ort_intra_threads(&prefs);
    if threads > 0 {
        builder = builder
            .with_intra_threads(threads)
            .map_err(|e| AppError::msg(format!("ort intra threads ({label}): {e}")))?;
    }

    builder.commit_from_file(path).map_err(|e| {
        AppError::msg(format!(
            "failed to load {label} model from {}: {e}",
            path.display()
        ))
    })
}
