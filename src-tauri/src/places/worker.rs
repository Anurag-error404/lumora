//! Background Places worker: reads GPS EXIF and reverse-geocodes offline.
//!
//! Unlike the ML workers this needs no model download and no network — the
//! reverse geocoder is bundled — so it always runs, backfilling existing assets
//! and picking up new imports. Every image gets a `places` job exactly once,
//! which makes the pass resumable and self-terminating.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use reverse_geocoder::ReverseGeocoder;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::places;
use crate::preferences;
use crate::state::open_db;

const BATCH: u32 = 32;
const IDLE_MS: u64 = 750;
const BETWEEN_MS: u64 = 5;

/// Process-wide reverse geocoder. Built once on first use (loads the bundled
/// GeoNames data and a k-d tree), then shared read-only.
static GEOCODER: OnceLock<ReverseGeocoder> = OnceLock::new();

pub fn geocoder() -> &'static ReverseGeocoder {
    GEOCODER.get_or_init(ReverseGeocoder::new)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacesProgress {
    pub pending: i64,
    pub done: i64,
    pub total: i64,
    pub running: bool,
    pub last_path: Option<String>,
}

pub struct PlacesWorker {
    db_path: PathBuf,
    app_data: PathBuf,
    running: AtomicBool,
    last_path: Mutex<Option<String>>,
    wake: AtomicBool,
}

impl PlacesWorker {
    pub fn new(db_path: PathBuf, app_data: PathBuf) -> Arc<Self> {
        let worker = Arc::new(Self {
            db_path,
            app_data,
            running: AtomicBool::new(false),
            last_path: Mutex::new(None),
            wake: AtomicBool::new(true),
        });
        let thread_worker = Arc::clone(&worker);
        thread::spawn(move || thread_worker.run_loop());
        worker
    }

    pub fn kick(&self) {
        self.wake.store(true, Ordering::Relaxed);
    }

    pub fn progress(&self) -> AppResult<PlacesProgress> {
        let conn = open_db(&self.db_path)?;
        let cov = places::coverage(&conn)?;
        let pending = cov.total.saturating_sub(cov.done);
        Ok(PlacesProgress {
            pending,
            done: cov.done,
            total: cov.total,
            running: self.running.load(Ordering::Relaxed),
            last_path: self.last_path.lock().clone(),
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
            if prefs.ai.background_processing == "paused" {
                self.running.store(false, Ordering::Relaxed);
                continue;
            }

            let worked = match self.drain_batch() {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "places batch failed");
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

    fn drain_batch(&self) -> AppResult<usize> {
        let conn = open_db(&self.db_path)?;
        let pending = places::pending_assets(&conn, BATCH)?;
        if pending.is_empty() {
            return Ok(0);
        }
        self.running.store(true, Ordering::Relaxed);

        let mut done = 0usize;
        for (id, path) in pending {
            *self.last_path.lock() = Some(path.clone());
            match places::extract_gps(Path::new(&path)) {
                Some((lat, lon)) => {
                    let (label, country) = places::reverse_geocode(lat, lon);
                    if let Err(e) = places::store_place(
                        &conn,
                        &id,
                        lat,
                        lon,
                        label.as_deref(),
                        country.as_deref(),
                    ) {
                        if e.is_db_busy() {
                            tracing::warn!(asset = %id, error = %e, "place store deferred (db busy)");
                        } else {
                            tracing::warn!(asset = %id, error = %e, "failed to store place");
                            let _ = places::mark_job(&conn, &id, "failed", Some(&e.to_string()));
                        }
                    } else {
                        done += 1;
                    }
                }
                None => {
                    // No GPS is a successful pass: mark done so we never re-read it.
                    let _ = places::mark_job(&conn, &id, "done", None);
                    done += 1;
                }
            }
        }
        Ok(done)
    }
}
