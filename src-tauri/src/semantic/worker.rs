//! Background CLIP embedding worker.
//!
//! Mirrors the indexer queue: a dedicated thread drains pending assets, writes
//! vectors, and yields so the UI stays responsive. The worker refuses to start
//! when the model is missing — Phase 1 features keep working either way.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::ml::{self, catalog::ModelKind, clip::ClipEngine};
use crate::preferences;
use crate::prefs_runtime;
use crate::semantic::{self, IMAGE_MODEL_ID};
use crate::state::open_db;

const BATCH: u32 = 8;
/// Staggered cold-start — CLIP after places, before faces.
const COLD_START_GRACE_SECS: u64 = 18;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedProgress {
    pub pending: i64,
    pub embedded: i64,
    pub total: i64,
    /// Assets that failed 3 times and need an explicit retry.
    pub failed: i64,
    pub running: bool,
    /// True when the user paused this pipeline (independent of global AI pause).
    pub paused: bool,
    pub last_path: Option<String>,
    /// Most recent failure message (job or engine), if any.
    pub last_error: Option<String>,
    pub model_ready: bool,
}

pub struct EmbedWorker {
    db_path: PathBuf,
    app_data: PathBuf,
    engine: Mutex<Option<Arc<ClipEngine>>>,
    running: AtomicBool,
    /// User-requested pause for this pipeline only.
    paused: AtomicBool,
    processed: AtomicU64,
    last_path: Mutex<Option<String>>,
    last_error: Mutex<Option<String>>,
    /// Set when the user asks us to (re)start after installing models.
    wake: AtomicBool,
}

impl EmbedWorker {
    pub fn new(db_path: PathBuf, app_data: PathBuf) -> Arc<Self> {
        let worker = Arc::new(Self {
            db_path,
            app_data,
            engine: Mutex::new(None),
            running: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            processed: AtomicU64::new(0),
            last_path: Mutex::new(None),
            last_error: Mutex::new(None),
            wake: AtomicBool::new(false),
        });
        let thread_worker = Arc::clone(&worker);
        thread::spawn(move || thread_worker.run_loop());
        worker
    }

    /// Drop a cached engine so the next wake reloads from disk (after install
    /// or remove).
    pub fn invalidate(&self) {
        *self.engine.lock() = None;
        self.wake.store(true, Ordering::Relaxed);
        super::ann::mark_dirty();
    }

    pub fn kick(&self) {
        self.paused.store(false, Ordering::Relaxed);
        // Permanently-failed assets are excluded from the queue — reset them so
        // Resume actually has work to do.
        if let Ok(conn) = open_db(&self.db_path) {
            match ml::reset_failed_jobs(&conn, ModelKind::ClipImage.as_str()) {
                Ok(n) if n > 0 => {
                    tracing::info!(reset = n, "re-queued failed embedding jobs");
                    *self.last_error.lock() = None;
                }
                Err(e) => {
                    *self.last_error.lock() = Some(format!("failed to reset jobs: {e}"));
                }
                _ => {}
            }
        }
        self.wake.store(true, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
        self.running.store(false, Ordering::Relaxed);
    }

    /// Load (or reuse) the CLIP engine. Returns `None` when the model is not
    /// installed yet — callers should fall back to FTS-only search.
    pub fn engine(&self) -> AppResult<Option<Arc<ClipEngine>>> {
        self.ensure_engine()
    }

    pub fn progress(&self) -> AppResult<EmbedProgress> {
        let conn = open_db(&self.db_path)?;
        let (embedded, total) = semantic::coverage(&conn)?;
        let failures = ml::job_failure_stats(&conn, ModelKind::ClipImage.as_str())?;
        let remaining = total.saturating_sub(embedded);
        let pending = remaining.saturating_sub(failures.failed);
        let runtime_error = self.last_error.lock().clone();
        Ok(EmbedProgress {
            pending,
            embedded,
            total,
            failed: failures.failed,
            // Don't report running after the queue is empty (last-batch lag).
            running: pending > 0 && self.running.load(Ordering::Relaxed),
            paused: self.paused.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
            last_error: runtime_error.or(failures.last_error),
            model_ready: ml::semantic_ready_for(&conn, Some(&self.app_data))?,
        })
    }

    fn run_loop(self: Arc<Self>) {
        loop {
            if !self.wake.swap(false, Ordering::Relaxed) {
                let prefs = preferences::load(&self.app_data).unwrap_or_default();
                thread::sleep(Duration::from_millis(
                    prefs_runtime::throttle(&prefs.performance).idle_ms,
                ));
                // Periodically re-check for newly indexed photos even without a kick.
                if !self.paused.load(Ordering::Relaxed)
                    && prefs_runtime::past_ml_cold_start(COLD_START_GRACE_SECS)
                {
                    self.wake.store(true, Ordering::Relaxed);
                }
                continue;
            }

            if self.paused.load(Ordering::Relaxed) {
                self.running.store(false, Ordering::Relaxed);
                let prefs = preferences::load(&self.app_data).unwrap_or_default();
                thread::sleep(Duration::from_millis(
                    prefs_runtime::throttle(&prefs.performance).idle_ms,
                ));
                continue;
            }

            let prefs = match preferences::load(&self.app_data) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !prefs_runtime::should_run_background(&prefs) {
                self.running.store(false, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(
                    prefs_runtime::throttle(&prefs.performance).idle_ms,
                ));
                continue;
            }

            let engine = match self.ensure_engine() {
                Ok(Some(e)) => e,
                Ok(None) => {
                    self.running.store(false, Ordering::Relaxed);
                    continue;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "semantic embedder unavailable");
                    *self.last_error.lock() = Some(e.to_string());
                    self.running.store(false, Ordering::Relaxed);
                    continue;
                }
            };

            let worked = match self.drain_batch(&engine, &prefs) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "embedding batch failed");
                    *self.last_error.lock() = Some(e.to_string());
                    0
                }
            };

            if worked > 0 {
                // More work likely remains — stay awake.
                self.wake.store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(
                    prefs_runtime::throttle(&prefs.performance).between_ms,
                ));
            } else {
                self.running.store(false, Ordering::Relaxed);
            }
        }
    }

    fn ensure_engine(&self) -> AppResult<Option<Arc<ClipEngine>>> {
        {
            let guard = self.engine.lock();
            if let Some(existing) = guard.as_ref() {
                return Ok(Some(Arc::clone(existing)));
            }
        }

        let conn = open_db(&self.db_path)?;
        if !ml::semantic_ready_for(&conn, Some(&self.app_data))? {
            return Ok(None);
        }
        let paths = semantic::model_paths_for(&conn, Some(&self.app_data))?;
        tracing::info!("loading CLIP engine for background embedding");
        let engine = Arc::new(ClipEngine::load(&paths)?);
        *self.engine.lock() = Some(Arc::clone(&engine));
        Ok(Some(engine))
    }

    fn drain_batch(
        &self,
        engine: &ClipEngine,
        prefs: &preferences::Preferences,
    ) -> AppResult<usize> {
        let conn = open_db(&self.db_path)?;
        let pending = semantic::pending_assets(&conn, BATCH)?;
        if pending.is_empty() {
            return Ok(0);
        }
        self.running.store(true, Ordering::Relaxed);

        let mut done = 0usize;
        for (id, path) in pending {
            if self.paused.load(Ordering::Relaxed) {
                self.running.store(false, Ordering::Relaxed);
                break;
            }
            *self.last_path.lock() = Some(path.clone());
            match engine.embed_image_path(PathBuf::from(&path).as_path()) {
                Ok(embedding) => {
                    if let Err(e) = semantic::store(&conn, &id, IMAGE_MODEL_ID, &embedding) {
                        if e.is_db_busy() {
                            tracing::warn!(asset = %id, error = %e, "embedding store deferred (db busy)");
                        } else {
                            tracing::warn!(asset = %id, error = %e, "failed to store embedding");
                            let _ = semantic::mark_job(
                                &conn,
                                &id,
                                ModelKind::ClipImage,
                                "failed",
                                Some(&e.to_string()),
                            );
                        }
                    } else {
                        done += 1;
                        self.processed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    tracing::debug!(asset = %id, error = %e, "image embed skipped");
                    *self.last_error.lock() = Some(format!("{}: {e}", path));
                    let _ = semantic::mark_job(
                        &conn,
                        &id,
                        ModelKind::ClipImage,
                        "failed",
                        Some(&e.to_string()),
                    );
                }
            }
            thread::sleep(Duration::from_millis(
                prefs_runtime::throttle(&prefs.performance).between_ms,
            ));
        }
        Ok(done)
    }
}

/// Convenience for commands that need a one-shot text embedding.
pub fn embed_query(engine: &ClipEngine, text: &str) -> AppResult<Vec<f32>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("semantic query is empty"));
    }
    engine.embed_text(trimmed)
}
