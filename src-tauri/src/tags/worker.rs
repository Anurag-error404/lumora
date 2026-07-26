//! Background MobileNetV4 auto-tag worker.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::preferences;
use crate::tags::{self, engine::TagsEngine};

const BATCH: u32 = 8;
const IDLE_MS: u64 = 750;
const BETWEEN_MS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagsProgress {
    pub pending: i64,
    pub done: i64,
    pub total: i64,
    pub running: bool,
    pub last_path: Option<String>,
    pub model_ready: bool,
}

pub struct TagsWorker {
    db_path: PathBuf,
    app_data: PathBuf,
    engine: Mutex<Option<Arc<TagsEngine>>>,
    running: AtomicBool,
    last_path: Mutex<Option<String>>,
    wake: AtomicBool,
}

impl TagsWorker {
    pub fn new(db_path: PathBuf, app_data: PathBuf) -> Arc<Self> {
        let worker = Arc::new(Self {
            db_path,
            app_data,
            engine: Mutex::new(None),
            running: AtomicBool::new(false),
            last_path: Mutex::new(None),
            wake: AtomicBool::new(true),
        });
        let thread_worker = Arc::clone(&worker);
        thread::spawn(move || thread_worker.run_loop());
        worker
    }

    pub fn invalidate(&self) {
        *self.engine.lock() = None;
        self.wake.store(true, Ordering::Relaxed);
    }

    pub fn kick(&self) {
        self.wake.store(true, Ordering::Relaxed);
    }

    pub fn progress(&self) -> AppResult<TagsProgress> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let cov = tags::coverage(&conn)?;
        let pending = cov.total.saturating_sub(cov.done);
        let bundle = tags::active_bundle(&self.app_data);
        Ok(TagsProgress {
            pending,
            done: cov.done,
            total: cov.total,
            running: self.running.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
            model_ready: tags::tags_ready_bundle(&conn, &bundle)?,
        })
    }

    fn run_loop(self: Arc<Self>) {
        loop {
            if !self.wake.swap(false, Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(IDLE_MS));
                self.wake.store(true, Ordering::Relaxed);
                continue;
            }

            let prefs = match preferences::load(&self.app_data) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !prefs.ai.object_detection {
                self.running.store(false, Ordering::Relaxed);
                continue;
            }

            let engine = match self.ensure_engine() {
                Ok(Some(e)) => e,
                Ok(None) => {
                    self.running.store(false, Ordering::Relaxed);
                    continue;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "auto-tags engine unavailable");
                    self.running.store(false, Ordering::Relaxed);
                    continue;
                }
            };

            let worked = match self.drain_batch(&engine) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "auto-tags batch failed");
                    0
                }
            };

            if worked > 0 {
                self.wake.store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(BETWEEN_MS));
            } else {
                self.running.store(false, Ordering::Relaxed);
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

        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let opt = tags::active_option(&self.app_data);
        let bundle = opt.bundle.unwrap_or(crate::ml::catalog::TAGS_BUNDLE);
        if !tags::tags_ready_bundle(&conn, bundle)? {
            return Ok(None);
        }
        let input_size = opt.input_size.unwrap_or(224);
        let paths = tags::model_paths_for(&conn, bundle, input_size)?;
        tracing::info!(bundle, input_size, "loading MobileNetV4 for auto-tags");
        let engine = Arc::new(TagsEngine::load(&paths)?);
        *self.engine.lock() = Some(Arc::clone(&engine));
        Ok(Some(engine))
    }

    fn drain_batch(&self, engine: &TagsEngine) -> AppResult<usize> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let pending = tags::pending_assets(&conn, BATCH)?;
        if pending.is_empty() {
            return Ok(0);
        }
        self.running.store(true, Ordering::Relaxed);

        let mut done = 0usize;
        for (id, path) in pending {
            *self.last_path.lock() = Some(path.clone());
            match engine.run_path(std::path::Path::new(&path)) {
                Ok(labels) => {
                    let model_id = tags::active_option(&self.app_data).id;
                    if let Err(e) = tags::store(&conn, &id, &labels, model_id) {
                        tracing::warn!(asset = %id, error = %e, "failed to store auto-tags");
                        let _ = tags::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                    } else {
                        done += 1;
                    }
                }
                Err(e) => {
                    tracing::debug!(asset = %id, error = %e, "auto-tags skipped");
                    let _ = tags::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                }
            }
            thread::sleep(Duration::from_millis(BETWEEN_MS));
        }
        Ok(done)
    }
}
