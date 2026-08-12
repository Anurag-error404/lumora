//! Background OCR worker.
//!
//! Mirrors the CLIP embedder: a dedicated thread drains pending images when
//! models are installed and the user has OCR enabled in preferences.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::ocr::{self, engine::OcrEngine};
use crate::preferences;
use crate::prefs_runtime;
use crate::state::open_db;

const BATCH: u32 = 4;
const COLD_START_GRACE_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrProgress {
    pub pending: i64,
    pub done: i64,
    pub total: i64,
    pub failed: i64,
    pub running: bool,
    /// True when the user paused this pipeline (independent of global AI pause).
    pub paused: bool,
    pub last_path: Option<String>,
    pub last_error: Option<String>,
    pub model_ready: bool,
}

pub struct OcrWorker {
    db_path: PathBuf,
    app_data: PathBuf,
    engine: Mutex<Option<Arc<OcrEngine>>>,
    running: AtomicBool,
    paused: AtomicBool,
    last_path: Mutex<Option<String>>,
    last_error: Mutex<Option<String>>,
    wake: AtomicBool,
}

impl OcrWorker {
    pub fn new(db_path: PathBuf, app_data: PathBuf) -> Arc<Self> {
        let worker = Arc::new(Self {
            db_path,
            app_data,
            engine: Mutex::new(None),
            running: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            last_path: Mutex::new(None),
            last_error: Mutex::new(None),
            wake: AtomicBool::new(false),
        });
        let thread_worker = Arc::clone(&worker);
        thread::spawn(move || thread_worker.run_loop());
        worker
    }

    pub fn invalidate(&self) {
        *self.engine.lock() = None;
        self.wake.store(true, Ordering::Relaxed);
    }

    pub fn unload_engine(&self) {
        *self.engine.lock() = None;
    }

    pub fn kick(&self) {
        self.paused.store(false, Ordering::Relaxed);
        if let Ok(conn) = open_db(&self.db_path) {
            match crate::ml::reset_failed_jobs(&conn, crate::ml::catalog::ModelKind::Ocr.as_str()) {
                Ok(n) if n > 0 => {
                    tracing::info!(reset = n, "re-queued failed OCR jobs");
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

    pub fn progress(&self) -> AppResult<OcrProgress> {
        let conn = open_db(&self.db_path)?;
        let cov = ocr::coverage(&conn)?;
        let failures =
            crate::ml::job_failure_stats(&conn, crate::ml::catalog::ModelKind::Ocr.as_str())?;
        let remaining = cov.total.saturating_sub(cov.done);
        let pending = remaining.saturating_sub(failures.failed);
        let bundle = ocr::active_bundle(&self.app_data);
        let runtime_error = self.last_error.lock().clone();
        Ok(OcrProgress {
            pending,
            done: cov.done,
            total: cov.total,
            failed: failures.failed,
            running: pending > 0 && self.running.load(Ordering::Relaxed),
            paused: self.paused.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
            last_error: runtime_error.or(failures.last_error),
            model_ready: ocr::ocr_ready_bundle(&conn, &bundle)?,
        })
    }

    fn run_loop(self: Arc<Self>) {
        loop {
            if !self.wake.swap(false, Ordering::Relaxed) {
                let prefs = preferences::load(&self.app_data).unwrap_or_default();
                thread::sleep(Duration::from_millis(
                    prefs_runtime::throttle(&prefs.performance).idle_ms,
                ));
                if !self.paused.load(Ordering::Relaxed)
                    && prefs_runtime::past_ml_cold_start(COLD_START_GRACE_SECS)
                {
                    self.wake.store(true, Ordering::Relaxed);
                }
                continue;
            }

            if self.paused.load(Ordering::Relaxed) {
                self.running.store(false, Ordering::Relaxed);
                self.unload_engine();
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
            if !prefs.ai.ocr || !prefs_runtime::should_run_background(&prefs) {
                self.running.store(false, Ordering::Relaxed);
                self.unload_engine();
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
                    tracing::warn!(error = %e, "OCR engine unavailable");
                    *self.last_error.lock() = Some(e.to_string());
                    self.running.store(false, Ordering::Relaxed);
                    continue;
                }
            };

            let worked = match self.drain_batch(&engine, &prefs) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "OCR batch failed");
                    *self.last_error.lock() = Some(e.to_string());
                    0
                }
            };

            if worked > 0 {
                self.wake.store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(
                    prefs_runtime::throttle(&prefs.performance).between_ms,
                ));
            } else {
                self.running.store(false, Ordering::Relaxed);
                drop(engine);
                self.unload_engine();
            }
        }
    }

    fn ensure_engine(&self) -> AppResult<Option<Arc<OcrEngine>>> {
        {
            let guard = self.engine.lock();
            if let Some(existing) = guard.as_ref() {
                return Ok(Some(Arc::clone(existing)));
            }
        }

        let conn = open_db(&self.db_path)?;
        let bundle = ocr::active_bundle(&self.app_data);
        if !ocr::ocr_ready_bundle(&conn, &bundle)? {
            return Ok(None);
        }
        let paths = ocr::model_paths_for(&conn, &bundle)?;
        tracing::info!(bundle = %bundle, "loading OCR engine for background text extraction");
        let engine = Arc::new(OcrEngine::load(&paths)?);
        *self.engine.lock() = Some(Arc::clone(&engine));
        Ok(Some(engine))
    }

    fn drain_batch(
        &self,
        engine: &OcrEngine,
        prefs: &preferences::Preferences,
    ) -> AppResult<usize> {
        let conn = open_db(&self.db_path)?;
        let pending = ocr::pending_assets(&conn, BATCH)?;
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
            match engine.run_path(std::path::Path::new(&path)) {
                Ok(result) => {
                    // Empty text is still a successful OCR pass — mark done so
                    // we don't loop forever on photos with no readable text.
                    if let Err(e) = ocr::store(&conn, &id, &result.text, result.confidence, None) {
                        if e.is_db_busy() {
                            tracing::warn!(asset = %id, error = %e, "OCR store deferred (db busy)");
                        } else {
                            tracing::warn!(asset = %id, error = %e, "failed to store OCR text");
                            let _ = ocr::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                        }
                    } else {
                        done += 1;
                    }
                }
                Err(e) => {
                    tracing::debug!(asset = %id, error = %e, "OCR skipped");
                    *self.last_error.lock() = Some(format!("{}: {e}", path));
                    let _ = ocr::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                }
            }
            thread::sleep(Duration::from_millis(
                prefs_runtime::throttle(&prefs.performance).between_ms,
            ));
        }
        Ok(done)
    }
}
