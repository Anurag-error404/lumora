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
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::ml::{self, catalog::ModelKind, clip::ClipEngine};
use crate::semantic::{self, IMAGE_MODEL_ID};

const BATCH: u32 = 8;
const IDLE_MS: u64 = 500;
const BETWEEN_MS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedProgress {
    pub pending: i64,
    pub embedded: i64,
    pub total: i64,
    pub running: bool,
    pub last_path: Option<String>,
    pub model_ready: bool,
}

pub struct EmbedWorker {
    db_path: PathBuf,
    engine: Mutex<Option<Arc<ClipEngine>>>,
    running: AtomicBool,
    processed: AtomicU64,
    last_path: Mutex<Option<String>>,
    /// Set when the user asks us to (re)start after installing models.
    wake: AtomicBool,
}

impl EmbedWorker {
    pub fn new(db_path: PathBuf, _models_dir: PathBuf) -> Arc<Self> {
        let worker = Arc::new(Self {
            db_path,
            engine: Mutex::new(None),
            running: AtomicBool::new(false),
            processed: AtomicU64::new(0),
            last_path: Mutex::new(None),
            wake: AtomicBool::new(true),
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
    }

    pub fn kick(&self) {
        self.wake.store(true, Ordering::Relaxed);
    }

    /// Load (or reuse) the CLIP engine. Returns `None` when the model is not
    /// installed yet — callers should fall back to FTS-only search.
    pub fn engine(&self) -> AppResult<Option<Arc<ClipEngine>>> {
        self.ensure_engine()
    }

    pub fn progress(&self) -> AppResult<EmbedProgress> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let (embedded, total) = semantic::coverage(&conn)?;
        let pending = total.saturating_sub(embedded);
        Ok(EmbedProgress {
            pending,
            embedded,
            total,
            running: self.running.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
            model_ready: ml::semantic_ready(&conn)?,
        })
    }

    fn run_loop(self: Arc<Self>) {
        loop {
            if !self.wake.swap(false, Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(IDLE_MS));
                // Periodically re-check for newly indexed photos even without a kick.
                self.wake.store(true, Ordering::Relaxed);
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
                    self.running.store(false, Ordering::Relaxed);
                    continue;
                }
            };

            let worked = match self.drain_batch(&engine) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "embedding batch failed");
                    0
                }
            };

            if worked > 0 {
                // More work likely remains — stay awake.
                self.wake.store(true, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(BETWEEN_MS));
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

        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        if !ml::semantic_ready(&conn)? {
            return Ok(None);
        }
        let paths = semantic::model_paths(&conn)?;
        tracing::info!("loading CLIP engine for background embedding");
        let engine = Arc::new(ClipEngine::load(&paths)?);
        *self.engine.lock() = Some(Arc::clone(&engine));
        Ok(Some(engine))
    }

    fn drain_batch(&self, engine: &ClipEngine) -> AppResult<usize> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let pending = semantic::pending_assets(&conn, BATCH)?;
        if pending.is_empty() {
            return Ok(0);
        }
        self.running.store(true, Ordering::Relaxed);

        let mut done = 0usize;
        for (id, path) in pending {
            *self.last_path.lock() = Some(path.clone());
            match engine.embed_image_path(PathBuf::from(&path).as_path()) {
                Ok(embedding) => {
                    if let Err(e) = semantic::store(&conn, &id, IMAGE_MODEL_ID, &embedding) {
                        tracing::warn!(asset = %id, error = %e, "failed to store embedding");
                        let _ = semantic::mark_job(
                            &conn,
                            &id,
                            ModelKind::ClipImage,
                            "failed",
                            Some(&e.to_string()),
                        );
                    } else {
                        done += 1;
                        self.processed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    tracing::debug!(asset = %id, error = %e, "image embed skipped");
                    let _ = semantic::mark_job(
                        &conn,
                        &id,
                        ModelKind::ClipImage,
                        "failed",
                        Some(&e.to_string()),
                    );
                }
            }
            thread::sleep(Duration::from_millis(BETWEEN_MS));
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
