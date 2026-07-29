//! User preferences persisted as JSON under app data.
//!
//! Preferences are configuration the person can change. Diagnostics stay on the
//! Developer page. Unknown keys from older/newer files are ignored via serde
//! defaults so the file can evolve without migrations.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const FILE_NAME: &str = "preferences.json";

static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Remember the app-data directory so ONNX session builders can read prefs
/// without every caller threading the path through.
pub fn set_app_data_dir(path: PathBuf) {
    let _ = APP_DATA_DIR.set(path);
}

/// Load preferences for the running app, or defaults when unset / unreadable.
pub fn load_current() -> Preferences {
    APP_DATA_DIR
        .get()
        .and_then(|p| load(p).ok())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub general: GeneralPrefs,
    pub appearance: AppearancePrefs,
    pub library: LibraryPrefs,
    pub ai: AiPrefs,
    pub privacy: PrivacyPrefs,
    pub performance: PerformancePrefs,
    pub import_export: ImportExportPrefs,
    pub updates: UpdatesPrefs,
    #[serde(default)]
    pub plugins: PluginsPrefs,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            general: GeneralPrefs::default(),
            appearance: AppearancePrefs::default(),
            library: LibraryPrefs::default(),
            ai: AiPrefs::default(),
            privacy: PrivacyPrefs::default(),
            performance: PerformancePrefs::default(),
            import_export: ImportExportPrefs::default(),
            updates: UpdatesPrefs::default(),
            plugins: PluginsPrefs::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneralPrefs {
    pub restore_previous_session: bool,
    pub double_click_opens_viewer: bool,
    pub confirm_before_deleting: bool,
    pub reveal_imported_photos: bool,
    pub language: String,
    pub date_format: String,
}

impl Default for GeneralPrefs {
    fn default() -> Self {
        Self {
            restore_previous_session: true,
            // False preserves single-click-to-open (current grid behaviour).
            double_click_opens_viewer: false,
            confirm_before_deleting: true,
            reveal_imported_photos: true,
            language: "en".into(),
            date_format: "dd/mm/yyyy".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppearancePrefs {
    /// "light" is the only supported value. Dark / system themes are out of scope.
    pub theme: String,
    pub accent: String,
    pub thumbnail_size: u32,
    pub density: String,
    pub animations: bool,
    pub smooth_scrolling: bool,
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            accent: "sage".into(),
            thumbnail_size: 160,
            density: "comfortable".into(),
            animations: true,
            smooth_scrolling: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPrefs {
    pub watch_folders_enabled: bool,
    /// `manual` | `on_launch` | `hourly` | `daily`
    pub auto_scan: String,
    /// Path/name patterns to skip during import and folder watching.
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

impl Default for LibraryPrefs {
    fn default() -> Self {
        Self {
            watch_folders_enabled: true,
            auto_scan: "manual".into(),
            ignore_patterns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiPrefs {
    pub semantic_search: bool,
    pub face_recognition: bool,
    pub object_detection: bool,
    pub ocr: bool,
    #[serde(default)]
    pub captions: bool,
    pub auto_albums: bool,
    pub processing_device: String,
    pub background_processing: String,
    /// Active library option id for semantic search (e.g. `clip-vit-b32`).
    #[serde(default = "default_semantic_model")]
    pub semantic_model: String,
    /// Active library option id for OCR.
    #[serde(default = "default_ocr_model")]
    pub ocr_model: String,
    /// Active library option id for faces.
    #[serde(default = "default_faces_model")]
    pub faces_model: String,
    /// Active library option id for auto-tags.
    #[serde(default = "default_tags_model")]
    pub tags_model: String,
    /// Active library option id for image captions.
    #[serde(default = "default_captions_model")]
    pub captions_model: String,
    /// Active library option id for memory prose.
    #[serde(default = "default_prose_model")]
    pub prose_model: String,
    /// When true and the prose model is installed, memories get a polished sentence.
    #[serde(default)]
    pub memory_prose: bool,
}

fn default_semantic_model() -> String {
    "clip-vit-b32".into()
}
fn default_ocr_model() -> String {
    "rapidocr-ppv4".into()
}
fn default_faces_model() -> String {
    "insightface-buffalo-l".into()
}
fn default_tags_model() -> String {
    "mobilenetv4-small".into()
}
fn default_captions_model() -> String {
    "florence-2-base-ft".into()
}
fn default_prose_model() -> String {
    "lamini-flan-t5-248m".into()
}

impl Default for AiPrefs {
    fn default() -> Self {
        Self {
            semantic_search: true,
            face_recognition: false,
            object_detection: false,
            ocr: false,
            captions: false,
            auto_albums: false,
            processing_device: "automatic".into(),
            background_processing: "always".into(),
            semantic_model: default_semantic_model(),
            ocr_model: default_ocr_model(),
            faces_model: default_faces_model(),
            tags_model: default_tags_model(),
            captions_model: default_captions_model(),
            prose_model: default_prose_model(),
            memory_prose: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyPrefs {
    pub auto_lock_minutes: u32,
    pub preserve_gps: bool,
    pub preserve_exif: bool,
    pub strip_metadata_on_export: bool,
}

impl Default for PrivacyPrefs {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 0,
            preserve_gps: true,
            preserve_exif: true,
            strip_metadata_on_export: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PerformancePrefs {
    pub cpu_profile: String,
    pub pause_on_battery: bool,
    pub thumbnail_cache_mb: u32,
}

impl Default for PerformancePrefs {
    fn default() -> Self {
        Self {
            cpu_profile: "balanced".into(),
            pause_on_battery: true,
            thumbnail_cache_mb: 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportExportPrefs {
    pub skip_duplicates: bool,
    pub preserve_folder_structure: bool,
    pub jpeg_quality: u8,
    pub strip_metadata: bool,
    /// Max long edge for exported stills; `0` keeps original size.
    #[serde(default)]
    pub export_max_edge: u32,
    /// `original` | `date_filename` | `sequential`
    #[serde(default = "default_export_naming")]
    pub export_naming: String,
}

fn default_export_naming() -> String {
    "original".into()
}

impl Default for ImportExportPrefs {
    fn default() -> Self {
        Self {
            skip_duplicates: true,
            preserve_folder_structure: true,
            jpeg_quality: 95,
            strip_metadata: false,
            export_max_edge: 0,
            export_naming: default_export_naming(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesPrefs {
    pub check_automatically: bool,
    pub download_in_background: bool,
}

impl Default for UpdatesPrefs {
    fn default() -> Self {
        Self {
            check_automatically: true,
            download_in_background: false,
        }
    }
}

/// Per-plugin enabled toggle; keyed by plugin id (reverse-DNS string).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginsPrefs {
    /// `true` = enabled, absent or `false` = disabled.
    #[serde(default)]
    pub enabled: std::collections::HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSummary {
    pub database_bytes: u64,
    pub thumbnail_bytes: u64,
    pub thumbnail_count: u64,
    pub models_bytes: u64,
    pub embeddings_bytes: u64,
    pub logs_bytes: u64,
    pub app_data_path: String,
    pub thumbs_path: String,
    pub models_path: String,
    pub logs_path: String,
    pub database_path: String,
}

pub fn prefs_path(app_data: &Path) -> PathBuf {
    app_data.join(FILE_NAME)
}

pub fn load(app_data: &Path) -> AppResult<Preferences> {
    let path = prefs_path(app_data);
    if !path.exists() {
        return Ok(Preferences::default());
    }
    let raw = fs::read_to_string(&path)?;
    match serde_json::from_str(&raw) {
        Ok(prefs) => Ok(prefs),
        Err(e) => {
            tracing::warn!(error = %e, "preferences.json unreadable; using defaults");
            Ok(Preferences::default())
        }
    }
}

pub fn save(app_data: &Path, prefs: &Preferences) -> AppResult<()> {
    let path = prefs_path(app_data);
    let raw = serde_json::to_string_pretty(prefs)
        .map_err(|e| AppError::msg(format!("serialize preferences: {e}")))?;
    fs::write(path, raw)?;
    Ok(())
}

pub fn storage_summary(
    conn: &Connection,
    app_data: &Path,
    db_path: &Path,
    thumbs_dir: &Path,
    models_dir: &Path,
    logs_dir: &Path,
) -> AppResult<StorageSummary> {
    Ok(StorageSummary {
        database_bytes: file_size(db_path),
        thumbnail_bytes: dir_size(thumbs_dir),
        thumbnail_count: count_files(thumbs_dir),
        models_bytes: dir_size(models_dir),
        embeddings_bytes: embeddings_bytes(conn)?,
        logs_bytes: dir_size(logs_dir),
        app_data_path: app_data.display().to_string(),
        thumbs_path: thumbs_dir.display().to_string(),
        models_path: models_dir.display().to_string(),
        logs_path: logs_dir.display().to_string(),
        database_path: db_path.display().to_string(),
    })
}

fn embeddings_bytes(conn: &Connection) -> AppResult<u64> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='asset_embeddings'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !exists {
        return Ok(0);
    }
    let bytes: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(LENGTH(vector)), 0) FROM asset_embeddings",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(bytes.max(0) as u64)
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

fn count_files(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trips_preferences_json() {
        let dir = tempdir().unwrap();
        let mut prefs = Preferences::default();
        prefs.appearance.thumbnail_size = 200;
        prefs.ai.semantic_search = false;
        save(dir.path(), &prefs).unwrap();
        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded.appearance.thumbnail_size, 200);
        assert!(!loaded.ai.semantic_search);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempdir().unwrap();
        let prefs = load(dir.path()).unwrap();
        assert!(prefs.general.confirm_before_deleting);
        assert_eq!(prefs.appearance.theme, "light");
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = tempdir().unwrap();
        fs::write(prefs_path(dir.path()), "{not json").unwrap();
        let prefs = load(dir.path()).unwrap();
        assert_eq!(prefs, Preferences::default());
    }
}
