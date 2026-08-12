//! Background Florence-2 caption worker.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::captions::{self, engine::CaptionsEngine};
use crate::error::AppResult;
use crate::preferences;
use crate::prefs_runtime;
use crate::state::open_db;

const BATCH: u32 = 4;
const COLD_START_GRACE_SECS: u64 = 42;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionsProgress {
    pub pending: i64,
    pub done: i64,
    pub total: i64,
    pub failed: i64,
    pub running: bool,
    pub paused: bool,
    pub last_path: Option<String>,
    pub last_error: Option<String>,
    pub model_ready: bool,
}

pub struct CaptionsWorker {
    db_path: PathBuf,
    app_data: PathBuf,
    engine: Mutex<Option<Arc<CaptionsEngine>>>,
    running: AtomicBool,
    paused: AtomicBool,
    last_path: Mutex<Option<String>>,
    last_error: Mutex<Option<String>>,
    wake: AtomicBool,
}

impl CaptionsWorker {
    pub fn new(db_path: PathBuf, app_data: PathBuf) -> Arc<Self> {
        let worker = Arc::new(Self {
            db_path, app_data, engine: Mutex::new(None), running: AtomicBool::new(false),
            paused: AtomicBool::new(false), last_path: Mutex::new(None), last_error: Mutex::new(None),
            wake: AtomicBool::new(false),
        });
        let thread_worker = Arc::clone(&worker);
        thread::spawn(move || thread_worker.run_loop());
        worker
    }

    pub fn invalidate(&self) { *self.engine.lock() = None; self.wake.store(true, Ordering::Relaxed); }
    pub fn pause(&self) { self.paused.store(true, Ordering::Relaxed); self.running.store(false, Ordering::Relaxed); }
    pub fn kick(&self) {
        self.paused.store(false, Ordering::Relaxed);
        if let Ok(conn) = open_db(&self.db_path) {
            if let Err(e) = crate::ml::reset_failed_jobs(&conn, crate::ml::catalog::ModelKind::Captions.as_str()) {
                *self.last_error.lock() = Some(format!("failed to reset caption jobs: {e}"));
            } else { *self.last_error.lock() = None; }
        }
        self.wake.store(true, Ordering::Relaxed);
    }

    pub fn progress(&self) -> AppResult<CaptionsProgress> {
        let conn = open_db(&self.db_path)?;
        let coverage = captions::coverage(&conn)?;
        let failures = crate::ml::job_failure_stats(&conn, crate::ml::catalog::ModelKind::Captions.as_str())?;
        let remaining = coverage.total.saturating_sub(coverage.done);
        let pending = remaining.saturating_sub(failures.failed);
        let bundle = captions::active_bundle(&self.app_data);
        Ok(CaptionsProgress {
            pending, done: coverage.done, total: coverage.total, failed: failures.failed,
            running: pending > 0 && self.running.load(Ordering::Relaxed),
            paused: self.paused.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
            last_error: self.last_error.lock().clone().or(failures.last_error),
            model_ready: captions::captions_ready_bundle(&conn, &bundle)?,
        })
    }

    fn run_loop(self: Arc<Self>) {
        loop {
            let prefs = preferences::load(&self.app_data).unwrap_or_default();
            let throttle = prefs_runtime::throttle(&prefs.performance);
            if !self.wake.swap(false, Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(throttle.idle_ms));
                if !self.paused.load(Ordering::Relaxed)
                    && prefs_runtime::past_ml_cold_start(COLD_START_GRACE_SECS)
                {
                    self.wake.store(true, Ordering::Relaxed);
                }
                continue;
            }
            if self.paused.load(Ordering::Relaxed) {
                self.running.store(false, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(throttle.idle_ms));
                continue;
            }
            if !prefs.ai.captions || !prefs_runtime::should_run_background(&prefs) {
                self.running.store(false, Ordering::Relaxed);
                *self.last_error.lock() = Some(if prefs.ai.captions {
                    "Background AI processing is paused.".into()
                } else { "Image captions are turned off in Settings → AI Features.".into() });
                thread::sleep(Duration::from_millis(throttle.idle_ms));
                continue;
            }
            let engine = match self.ensure_engine() {
                Ok(Some(engine)) => engine,
                Ok(None) => { self.running.store(false, Ordering::Relaxed); *self.last_error.lock() = Some("Florence-2 models are not fully installed yet.".into()); thread::sleep(Duration::from_millis(throttle.idle_ms)); continue; }
                Err(e) => { self.running.store(false, Ordering::Relaxed); tracing::warn!(error=%e, "captions engine unavailable"); *self.last_error.lock() = Some(e.to_string()); thread::sleep(Duration::from_millis(throttle.idle_ms)); continue; }
            };
            match self.drain_batch(&engine, &prefs) {
                Ok(n) if n > 0 => { self.wake.store(true, Ordering::Relaxed); thread::sleep(Duration::from_millis(throttle.between_ms)); }
                Ok(_) => self.running.store(false, Ordering::Relaxed),
                Err(e) => { tracing::warn!(error=%e, "captions batch failed"); *self.last_error.lock() = Some(e.to_string()); self.running.store(false, Ordering::Relaxed); }
            }
        }
    }

    fn ensure_engine(&self) -> AppResult<Option<Arc<CaptionsEngine>>> {
        if let Some(engine) = self.engine.lock().as_ref() { return Ok(Some(Arc::clone(engine))); }
        let conn = open_db(&self.db_path)?;
        let option = captions::active_option(&self.app_data);
        let bundle = option.bundle.unwrap_or(crate::ml::catalog::CAPTIONS_BUNDLE);
        if !captions::captions_ready_bundle(&conn, bundle)? { return Ok(None); }
        let engine = Arc::new(CaptionsEngine::load(&captions::model_paths_for(&conn, bundle)?)?);
        *self.engine.lock() = Some(Arc::clone(&engine));
        Ok(Some(engine))
    }

    fn drain_batch(&self, engine: &CaptionsEngine, prefs: &preferences::Preferences) -> AppResult<usize> {
        let conn = open_db(&self.db_path)?;
        let pending = captions::pending_assets(&conn, BATCH)?;
        if pending.is_empty() { return Ok(0); }
        self.running.store(true, Ordering::Relaxed);
        let mut done = 0;
        for (id, path) in pending {
            if self.paused.load(Ordering::Relaxed) { self.running.store(false, Ordering::Relaxed); break; }
            *self.last_path.lock() = Some(path.clone());
            match engine.run_path(std::path::Path::new(&path)) {
                Ok(caption) => match captions::store(&conn, &id, &caption, captions::active_option(&self.app_data).id) {
                    Ok(()) => done += 1,
                    Err(e) => { tracing::warn!(asset=%id, error=%e, "caption store failed"); let _ = captions::mark_job(&conn, &id, "failed", Some(&e.to_string())); }
                },
                Err(e) => { tracing::debug!(asset=%id, error=%e, "caption skipped"); *self.last_error.lock() = Some(format!("{path}: {e}")); let _ = captions::mark_job(&conn, &id, "failed", Some(&e.to_string())); }
            }
            thread::sleep(Duration::from_millis(prefs_runtime::throttle(&prefs.performance).between_ms));
        }
        Ok(done)
    }
}
