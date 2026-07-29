use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::Connection;

use zeroize::Zeroize;

use crate::error::AppResult;
use crate::faces::worker::FaceWorker;
use crate::captions::worker::CaptionsWorker;
use crate::history::HistoryStacks;
use crate::indexer::queue::IndexerQueue;
use crate::ocr::worker::OcrWorker;
use crate::places::worker::PlacesWorker;
use crate::semantic::worker::EmbedWorker;
use crate::tags::worker::TagsWorker;

/// An unlocked vault session: holds the vault id and decrypted master key in
/// memory only. The key is wiped when the session is dropped (lock / app exit).
pub struct VaultSession {
    pub vault_id: String,
    pub master_key: [u8; 32],
}

impl Drop for VaultSession {
    fn drop(&mut self) {
        self.master_key.zeroize();
    }
}

#[derive(Clone)]
pub struct AppPaths {
    pub app_data: PathBuf,
    pub db_path: PathBuf,
    pub thumbs_dir: PathBuf,
    pub logs_dir: PathBuf,
    /// On-device ML models. User-visible and safe to delete: removing this
    /// directory only disables AI features, it never harms the library.
    pub models_dir: PathBuf,
    /// Face crop chips (separate from thumbs so cache clears don't wipe them).
    pub faces_dir: PathBuf,
}

impl AppPaths {
    pub fn from_app_data(app_data: PathBuf) -> AppResult<Self> {
        let thumbs_dir = app_data.join("thumbs");
        let logs_dir = app_data.join("logs");
        let models_dir = app_data.join("models");
        let faces_dir = app_data.join("faces");
        let db_path = app_data.join("library.db");
        std::fs::create_dir_all(&app_data)?;
        std::fs::create_dir_all(&thumbs_dir)?;
        std::fs::create_dir_all(&logs_dir)?;
        std::fs::create_dir_all(&models_dir)?;
        std::fs::create_dir_all(&faces_dir)?;
        Ok(Self {
            app_data,
            db_path,
            thumbs_dir,
            logs_dir,
            models_dir,
            faces_dir,
        })
    }

    #[cfg(test)]
    pub fn for_temp(tmp: &Path) -> AppResult<Self> {
        Self::from_app_data(tmp.to_path_buf())
    }
}

pub struct AppState {
    pub paths: AppPaths,
    pub db: Mutex<Connection>,
    pub indexer: Arc<IndexerQueue>,
    pub embedder: Arc<EmbedWorker>,
    pub ocr: Arc<OcrWorker>,
    pub faces: Arc<FaceWorker>,
    pub places: Arc<PlacesWorker>,
    pub tags: Arc<TagsWorker>,
    pub captions: Arc<CaptionsWorker>,
    pub history: Mutex<HistoryStacks>,
    /// Set to true by `cancel_import` to abort an in-flight import.
    pub import_cancel: Arc<AtomicBool>,
    /// `Some` while the privacy vault is unlocked for this session.
    pub vault: Mutex<Option<VaultSession>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        paths: AppPaths,
        db: Connection,
        indexer: Arc<IndexerQueue>,
        embedder: Arc<EmbedWorker>,
        ocr: Arc<OcrWorker>,
        faces: Arc<FaceWorker>,
        places: Arc<PlacesWorker>,
        tags: Arc<TagsWorker>,
        captions: Arc<CaptionsWorker>,
    ) -> Self {
        Self {
            paths,
            db: Mutex::new(db),
            indexer,
            embedder,
            ocr,
            faces,
            places,
            tags,
            captions,
            history: Mutex::new(HistoryStacks::default()),
            import_cancel: Arc::new(AtomicBool::new(false)),
            vault: Mutex::new(None),
        }
    }

    /// Active vault id + master key, or error if locked.
    pub fn vault_session(&self) -> AppResult<(String, [u8; 32])> {
        self.vault
            .lock()
            .as_ref()
            .map(|s| (s.vault_id.clone(), s.master_key))
            .ok_or_else(|| crate::error::AppError::msg("vault is locked"))
    }

    /// Require that the unlocked session matches `vault_id`.
    pub fn require_vault(&self, vault_id: &str) -> AppResult<[u8; 32]> {
        let (active_id, key) = self.vault_session()?;
        if active_id != vault_id {
            return Err(crate::error::AppError::msg(
                "unlock that vault first before moving items into it",
            ));
        }
        Ok(key)
    }

    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self.db.lock();
        f(&conn)
    }
}

/// Open a library DB connection with settings safe for concurrent workers.
///
/// WAL allows readers alongside a writer; `busy_timeout` makes competing
/// writers wait instead of immediately failing with "database is locked".
pub fn open_db(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;",
    )?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn logging_paths_resolvable() {
        let dir = tempdir().unwrap();
        let paths = AppPaths::for_temp(dir.path()).unwrap();
        assert!(paths.app_data.exists());
        assert!(paths.thumbs_dir.exists());
        assert!(paths.logs_dir.exists());
        assert!(paths.models_dir.exists());
        assert_eq!(paths.db_path.file_name().unwrap(), "library.db");
    }
}
