//! Background MobileNetV4 auto-tag worker.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::preferences;
use crate::prefs_runtime;
use crate::state::open_db;
use crate::tags::{self, engine::TagsEngine};

const BATCH: u32 = 8;
const COLD_START_GRACE_SECS: u64 = 36;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagsProgress {
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

pub struct TagsWorker {
    db_path: PathBuf,
    app_data: PathBuf,
    engine: Mutex<Option<Arc<TagsEngine>>>,
    running: AtomicBool,
    paused: AtomicBool,
    last_path: Mutex<Option<String>>,
    last_error: Mutex<Option<String>>,
    wake: AtomicBool,
}

impl TagsWorker {
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
            match crate::ml::reset_failed_jobs(&conn, crate::ml::catalog::ModelKind::Tags.as_str())
            {
                Ok(n) if n > 0 => {
                    tracing::info!(reset = n, "re-queued failed auto-tag jobs");
                    *self.last_error.lock() = None;
                }
                Err(e) => {
                    *self.last_error.lock() = Some(format!("failed to reset jobs: {e}"));
                }
                _ => {}
            }
            // Older runs marked empty classifications as done without labels —
            // clear those so Resume can store at least a top-1 label.
            match conn.execute(
                "DELETE FROM ml_jobs
                 WHERE kind = ?1
                   AND state = 'done'
                   AND NOT EXISTS (
                     SELECT 1 FROM asset_labels l WHERE l.asset_id = ml_jobs.asset_id
                   )",
                rusqlite::params![crate::ml::catalog::ModelKind::Tags.as_str()],
            ) {
                Ok(n) if n > 0 => {
                    tracing::info!(reset = n, "re-queued empty auto-tag jobs");
                }
                Err(e) => {
                    *self.last_error.lock() = Some(format!("failed to reset empty jobs: {e}"));
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

    pub fn progress(&self) -> AppResult<TagsProgress> {
        let conn = open_db(&self.db_path)?;
        let cov = tags::coverage(&conn)?;
        let failures =
            crate::ml::job_failure_stats(&conn, crate::ml::catalog::ModelKind::Tags.as_str())?;
        let remaining = cov.total.saturating_sub(cov.done);
        let pending = remaining.saturating_sub(failures.failed);
        let bundle = tags::active_bundle(&self.app_data);
        let runtime_error = self.last_error.lock().clone();
        Ok(TagsProgress {
            pending,
            done: cov.done,
            total: cov.total,
            failed: failures.failed,
            running: pending > 0 && self.running.load(Ordering::Relaxed),
            paused: self.paused.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
            last_error: runtime_error.or(failures.last_error),
            model_ready: tags::tags_ready_bundle(&conn, &bundle)?,
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
            if !prefs.ai.object_detection || !prefs_runtime::should_run_background(&prefs) {
                self.running.store(false, Ordering::Relaxed);
                self.unload_engine();
                if !prefs.ai.object_detection {
                    *self.last_error.lock() =
                        Some("Object detection is turned off in Settings → AI Features.".into());
                } else {
                    *self.last_error.lock() = Some("Background AI processing is paused.".into());
                }
                thread::sleep(Duration::from_millis(
                    prefs_runtime::throttle(&prefs.performance).idle_ms,
                ));
                continue;
            }

            let engine = match self.ensure_engine() {
                Ok(Some(e)) => e,
                Ok(None) => {
                    self.running.store(false, Ordering::Relaxed);
                    *self.last_error.lock() =
                        Some("Auto-tag models are not fully installed yet.".into());
                    thread::sleep(Duration::from_millis(
                        prefs_runtime::throttle(&prefs.performance).idle_ms,
                    ));
                    continue;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "auto-tags engine unavailable");
                    *self.last_error.lock() = Some(e.to_string());
                    self.running.store(false, Ordering::Relaxed);
                    thread::sleep(Duration::from_millis(
                        prefs_runtime::throttle(&prefs.performance).idle_ms,
                    ));
                    continue;
                }
            };

            let worked = match self.drain_batch(&engine, &prefs) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "auto-tags batch failed");
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

    fn ensure_engine(&self) -> AppResult<Option<Arc<TagsEngine>>> {
        {
            let guard = self.engine.lock();
            if let Some(existing) = guard.as_ref() {
                return Ok(Some(Arc::clone(existing)));
            }
        }

        let conn = open_db(&self.db_path)?;
        let preferred = tags::active_tags_model_id(&self.app_data);
        let paths = if crate::ml::user::is_user_option_id(&preferred) {
            let Some(uopt) = crate::ml::user::get(&conn, &preferred)? else {
                return Ok(None);
            };
            crate::ml::user::tags_paths(&uopt)?
        } else {
            let opt = tags::active_option(&self.app_data);
            let bundle = opt.bundle.unwrap_or(crate::ml::catalog::TAGS_BUNDLE);
            if !tags::tags_ready_bundle(&conn, bundle)? {
                return Ok(None);
            }
            let input_size = opt.input_size.unwrap_or(224);
            tags::model_paths_for(&conn, bundle, input_size)?
        };
        tracing::info!(
            model = preferred.as_str(),
            input_size = paths.input_size,
            "loading auto-tags classifier"
        );
        let engine = Arc::new(TagsEngine::load(&paths)?);
        *self.engine.lock() = Some(Arc::clone(&engine));
        Ok(Some(engine))
    }

    fn drain_batch(
        &self,
        engine: &TagsEngine,
        prefs: &preferences::Preferences,
    ) -> AppResult<usize> {
        let conn = open_db(&self.db_path)?;
        let pending = tags::pending_assets(
            &conn,
            prefs_runtime::scaled_batch(BATCH, &prefs.performance),
        )?;
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
                Ok(labels) => {
                    let model_id = tags::active_option(&self.app_data).id;
                    if let Err(e) = tags::store(&conn, &id, &labels, model_id) {
                        if e.is_db_busy() {
                            tracing::warn!(asset = %id, error = %e, "auto-tags store deferred (db busy)");
                        } else {
                            tracing::warn!(asset = %id, error = %e, "failed to store auto-tags");
                            let _ = tags::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                        }
                    } else {
                        done += 1;
                    }
                }
                Err(e) => {
                    tracing::debug!(asset = %id, error = %e, "auto-tags skipped");
                    *self.last_error.lock() = Some(format!("{}: {e}", path));
                    let _ = tags::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                }
            }
        }
        Ok(done)
    }
}
