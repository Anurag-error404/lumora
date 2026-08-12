use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;

use crate::error::AppResult;
use crate::indexer;
use crate::models::IndexProgress;
use crate::preferences;
use crate::prefs_runtime;
use crate::state::open_db;

#[derive(Debug, Clone)]
pub enum IndexJob {
    Upsert { path: PathBuf, generate_thumb: bool },
    SoftRemove { path: PathBuf },
}

pub struct IndexerQueue {
    queue: Mutex<VecDeque<IndexJob>>,
    processed: AtomicU64,
    running: AtomicBool,
    last_path: Mutex<Option<String>>,
    db_path: PathBuf,
    thumbs_dir: PathBuf,
}

impl IndexerQueue {
    pub fn new(db_path: PathBuf, thumbs_dir: PathBuf) -> Arc<Self> {
        let q = Arc::new(Self {
            queue: Mutex::new(VecDeque::new()),
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
        let mut q = self.queue.lock();
        match &job {
            IndexJob::Upsert {
                path,
                generate_thumb,
            } => {
                let mut want_thumb = *generate_thumb;
                q.retain(|existing| match existing {
                    IndexJob::Upsert {
                        path: p,
                        generate_thumb: t,
                    } if p == path => {
                        want_thumb |= *t;
                        false
                    }
                    IndexJob::SoftRemove { path: p } if p == path => false,
                    _ => true,
                });
                q.push_back(IndexJob::Upsert {
                    path: path.clone(),
                    generate_thumb: want_thumb,
                });
            }
            IndexJob::SoftRemove { path } => {
                q.retain(|existing| match existing {
                    IndexJob::Upsert { path: p, .. } | IndexJob::SoftRemove { path: p }
                        if p == path =>
                    {
                        false
                    }
                    _ => true,
                });
                q.push_back(job);
            }
        }
    }

    pub fn pending_len(&self) -> usize {
        self.queue.lock().len()
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
            let job = { self.queue.lock().pop_front() };
            match job {
                Some(job) => {
                    self.running.store(true, Ordering::Relaxed);
                    let _ = self.process(job);
                    self.processed.fetch_add(1, Ordering::Relaxed);
                    // Mild throttle so UI stays responsive on large imports.
                    let prefs = preferences::load_current();
                    thread::sleep(Duration::from_millis(
                        prefs_runtime::throttle(&prefs.performance).between_ms,
                    ));
                }
                None => {
                    self.running.store(false, Ordering::Relaxed);
                    let prefs = preferences::load_current();
                    thread::sleep(Duration::from_millis(
                        prefs_runtime::throttle(&prefs.performance).idle_ms,
                    ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn enqueue_is_fifo_and_dedupes_path() {
        let q = IndexerQueue {
            queue: Mutex::new(VecDeque::new()),
            processed: AtomicU64::new(0),
            running: AtomicBool::new(false),
            last_path: Mutex::new(None),
            db_path: PathBuf::from("/tmp"),
            thumbs_dir: PathBuf::from("/tmp"),
        };
        q.enqueue(IndexJob::Upsert {
            path: PathBuf::from("/a.jpg"),
            generate_thumb: false,
        });
        q.enqueue(IndexJob::Upsert {
            path: PathBuf::from("/b.jpg"),
            generate_thumb: false,
        });
        q.enqueue(IndexJob::Upsert {
            path: PathBuf::from("/a.jpg"),
            generate_thumb: true,
        });
        let locked = q.queue.lock();
        assert_eq!(locked.len(), 2);
        match &locked[0] {
            IndexJob::Upsert {
                path,
                generate_thumb,
            } => {
                assert_eq!(path, &PathBuf::from("/b.jpg"));
                assert!(!generate_thumb);
            }
            _ => panic!("expected upsert"),
        }
        match &locked[1] {
            IndexJob::Upsert {
                path,
                generate_thumb,
            } => {
                assert_eq!(path, &PathBuf::from("/a.jpg"));
                assert!(generate_thumb, "thumb flag must survive dedupe");
            }
            _ => panic!("expected upsert"),
        }
    }
}
