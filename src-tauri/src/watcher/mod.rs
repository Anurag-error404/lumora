use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::error::AppResult;
use crate::indexer;
use crate::indexer::queue::{IndexJob, IndexerQueue};

pub struct WatcherService {
    roots: Mutex<Vec<PathBuf>>,
}

impl WatcherService {
    pub fn new() -> Self {
        Self {
            roots: Mutex::new(Vec::new()),
        }
    }

    pub fn start(
        self: &std::sync::Arc<Self>,
        queue: std::sync::Arc<IndexerQueue>,
        initial_roots: Vec<PathBuf>,
    ) {
        *self.roots.lock() = initial_roots.clone();
        let service = std::sync::Arc::clone(self);
        thread::spawn(move || {
            let (tx, rx) = mpsc::channel();
            let mut watcher = match RecommendedWatcher::new(
                move |res| {
                    let _ = tx.send(res);
                },
                Config::default().with_poll_interval(Duration::from_secs(2)),
            ) {
                Ok(w) => w,
                Err(err) => {
                    tracing::error!(?err, "failed to start folder watcher");
                    return;
                }
            };

            let mut active: HashSet<PathBuf> = HashSet::new();
            for root in initial_roots {
                if let Err(err) = watcher.watch(&root, RecursiveMode::Recursive) {
                    tracing::warn!(?err, path = %root.display(), "watch failed");
                } else {
                    active.insert(root);
                }
            }

            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(Ok(event)) => handle_event(&queue, event),
                    Ok(Err(err)) => tracing::warn!(?err, "watch error"),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let desired: HashSet<PathBuf> =
                            service.roots.lock().iter().cloned().collect();
                        for path in active.difference(&desired) {
                            if let Err(err) = watcher.unwatch(path) {
                                tracing::warn!(
                                    ?err,
                                    path = %path.display(),
                                    "unwatch failed"
                                );
                            }
                        }
                        for path in desired.difference(&active) {
                            if let Err(err) = watcher.watch(path, RecursiveMode::Recursive) {
                                tracing::warn!(
                                    ?err,
                                    path = %path.display(),
                                    "watch failed"
                                );
                            }
                        }
                        active = desired;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
    }

    pub fn add_root(&self, path: PathBuf) {
        let mut roots = self.roots.lock();
        if !roots.iter().any(|p| p == &path) {
            roots.push(path);
        }
    }

    pub fn remove_root(&self, path: &Path) {
        self.roots.lock().retain(|p| p != path);
    }
}

fn handle_event(queue: &IndexerQueue, event: notify::Event) {
    let paths = event.paths;
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in paths {
                if path.is_file() && indexer::is_supported_media(&path) {
                    queue.enqueue(IndexJob::Upsert {
                        path,
                        generate_thumb: true,
                    });
                }
            }
        }
        EventKind::Remove(_) => {
            for path in paths {
                queue.enqueue(IndexJob::SoftRemove { path });
            }
        }
        _ => {}
    }
}

pub fn list_watched(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare("SELECT path FROM watched_folders ORDER BY created_at")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn add_watched(conn: &Connection, path: &Path) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let path_str = path.to_string_lossy().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO watched_folders (id, path, created_at) VALUES (?1, ?2, ?3)",
        params![id, path_str, now],
    )?;
    Ok(path_str)
}

pub fn remove_watched(conn: &Connection, path: &Path) -> AppResult<bool> {
    let path_str = path.to_string_lossy().to_string();
    let n = conn.execute(
        "DELETE FROM watched_folders WHERE path = ?1",
        params![path_str],
    )?;
    Ok(n > 0)
}

pub fn load_watched_paths(conn: &Connection) -> AppResult<Vec<PathBuf>> {
    Ok(list_watched(conn)?.into_iter().map(PathBuf::from).collect())
}
