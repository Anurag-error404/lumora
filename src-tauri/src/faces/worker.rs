//! Background face detection / embedding / clustering worker.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::faces::{self, engine::FaceEngine};
use crate::preferences;

const BATCH: u32 = 2;
const IDLE_MS: u64 = 750;
const BETWEEN_MS: u64 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacesProgress {
    pub pending: i64,
    pub done: i64,
    pub total: i64,
    pub running: bool,
    pub last_path: Option<String>,
    pub model_ready: bool,
}

pub struct FaceWorker {
    db_path: PathBuf,
    app_data: PathBuf,
    faces_dir: PathBuf,
    engine: Mutex<Option<Arc<FaceEngine>>>,
    running: AtomicBool,
    last_path: Mutex<Option<String>>,
    wake: AtomicBool,
}

impl FaceWorker {
    pub fn new(db_path: PathBuf, app_data: PathBuf) -> Arc<Self> {
        let faces_dir = app_data.join("faces");
        let _ = std::fs::create_dir_all(&faces_dir);
        let worker = Arc::new(Self {
            db_path,
            app_data,
            faces_dir,
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

    pub fn progress(&self) -> AppResult<FacesProgress> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let cov = faces::coverage(&conn)?;
        let pending = cov.total.saturating_sub(cov.done);
        Ok(FacesProgress {
            pending,
            done: cov.done,
            total: cov.total,
            running: self.running.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
            model_ready: faces::faces_ready(&conn)?,
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
            if !prefs.ai.face_recognition {
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
                    tracing::warn!(error = %e, "face engine unavailable");
                    self.running.store(false, Ordering::Relaxed);
                    continue;
                }
            };

            let worked = match self.drain_batch(&engine) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "faces batch failed");
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

    fn ensure_engine(&self) -> AppResult<Option<Arc<FaceEngine>>> {
        {
            let guard = self.engine.lock();
            if let Some(existing) = guard.as_ref() {
                return Ok(Some(Arc::clone(existing)));
            }
        }

        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        if !faces::faces_ready(&conn)? {
            return Ok(None);
        }
        let paths = faces::model_paths(&conn)?;
        tracing::info!("loading face engine for background detection");
        let engine = Arc::new(FaceEngine::load(&paths)?);
        *self.engine.lock() = Some(Arc::clone(&engine));
        Ok(Some(engine))
    }

    fn drain_batch(&self, engine: &FaceEngine) -> AppResult<usize> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let pending = faces::pending_assets(&conn, BATCH)?;
        if pending.is_empty() {
            return Ok(0);
        }
        self.running.store(true, Ordering::Relaxed);

        let mut done = 0usize;
        for (id, path) in pending {
            *self.last_path.lock() = Some(path.clone());
            match engine.run_path(std::path::Path::new(&path)) {
                Ok(detections) => {
                    if let Err(e) =
                        faces::store_detections(&conn, &self.faces_dir, &id, &detections)
                    {
                        tracing::warn!(asset = %id, error = %e, "failed to store faces");
                        let _ = faces::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                    } else {
                        done += 1;
                    }
                }
                Err(e) => {
                    tracing::debug!(asset = %id, error = %e, "faces skipped");
                    let _ = faces::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                }
            }
            thread::sleep(Duration::from_millis(BETWEEN_MS));
        }
        Ok(done)
    }
}
