use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;

use crate::error::AppResult;
use crate::indexer;
use crate::models::IndexProgress;
use crate::state::open_db;

#[derive(Debug, Clone)]
pub enum IndexJob {
    Upsert { path: PathBuf, generate_thumb: bool },
    SoftRemove { path: PathBuf },
}

pub struct IndexerQueue {
    queue: Mutex<Vec<IndexJob>>,
    processed: AtomicU64,
    running: AtomicBool,
    last_path: Mutex<Option<String>>,
    db_path: PathBuf,
    thumbs_dir: PathBuf,
}

impl IndexerQueue {
    pub fn new(db_path: PathBuf, thumbs_dir: PathBuf) -> Arc<Self> {
        let q = Arc::new(Self {
            queue: Mutex::new(Vec::new()),
            processed: AtomicU64::new(0),
            running: AtomicBool::new(false),
            last_path: Mutex::new(None),
            db_path,
            thumbs_dir,
        });
        let worker = Arc::clone(&q);
        thread::spawn(move || worker.run_loop());
        q
    }

    pub fn enqueue(&self, job: IndexJob) {
        self.queue.lock().push(job);
    }

    pub fn progress(&self) -> IndexProgress {
        IndexProgress {
            pending: self.queue.lock().len(),
            processed: self.processed.load(Ordering::Relaxed),
            running: self.running.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
        }
    }

    fn run_loop(self: Arc<Self>) {
        loop {
            let job = { self.queue.lock().pop() };
            match job {
                Some(job) => {
                    self.running.store(true, Ordering::Relaxed);
                    let _ = self.process(job);
                    self.processed.fetch_add(1, Ordering::Relaxed);
                    // Mild throttle so UI stays responsive on large imports.
                    thread::sleep(Duration::from_millis(2));
                }
                None => {
                    self.running.store(false, Ordering::Relaxed);
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }

    fn process(&self, job: IndexJob) -> AppResult<()> {
        let conn = open_db(&self.db_path)?;
        match job {
            IndexJob::Upsert {
                path,
                generate_thumb,
            } => {
                *self.last_path.lock() = Some(path.display().to_string());
                let _ = indexer::upsert_asset(&conn, &path, &self.thumbs_dir, generate_thumb)?;
            }
            IndexJob::SoftRemove { path } => {
                *self.last_path.lock() = Some(path.display().to_string());
                let _ = indexer::remove_asset_by_path(&conn, &path)?;
            }
        }
        Ok(())
    }
}
