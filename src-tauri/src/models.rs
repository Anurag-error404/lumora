use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSummary {
    pub id: String,
    pub path: String,
    pub hash: String,
    pub perceptual_hash: Option<String>,
    pub media_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
    pub captured_at: Option<String>,
    pub indexed_at: String,
    pub favorite: bool,
    pub rating: i64,
    pub color_label: Option<String>,
    pub thumbnail_path: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub total_assets: i64,
    pub total_images: i64,
    pub total_videos: i64,
    pub favorites: i64,
    pub in_trash: i64,
    pub album_count: i64,
    pub tag_count: i64,
    pub watched_folders: i64,
    pub trash_retention_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub name: String,
    pub cover_asset_id: Option<String>,
    pub cover_thumbnail_path: Option<String>,
    pub created_at: String,
    pub asset_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub asset_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSearch {
    pub id: String,
    pub name: String,
    pub query: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetOrganisation {
    pub albums: Vec<Album>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetCount {
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFacets {
    pub ratings: Vec<FacetCount>,
    pub color_labels: Vec<FacetCount>,
}

/// Multi-condition browse filter for the Tags page.
/// Within each list values are OR'd; across lists they are AND'd.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TagBrowseFilter {
    #[serde(default)]
    pub tag_ids: Vec<String>,
    #[serde(default)]
    pub ratings: Vec<i64>,
    #[serde(default)]
    pub color_labels: Vec<String>,
}

impl TagBrowseFilter {
    pub fn is_empty(&self) -> bool {
        self.tag_ids.is_empty() && self.ratings.is_empty() && self.color_labels.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineMonth {
    pub year: i32,
    pub month: u32,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub pending: usize,
    pub processed: u64,
    pub running: bool,
    pub last_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub kind: String,
    pub key: String,
    pub asset_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlurryAsset {
    pub asset: AssetSummary,
    pub blur_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub scanned: u64,
    pub inserted: u64,
    pub updated: u64,
    pub skipped: u64,
    /// True when the user aborted mid-import; counts reflect work already done.
    #[serde(default)]
    pub cancelled: bool,
    /// Wall-clock duration of the import job in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Effective throughput (`scanned / duration`), zero when duration is 0.
    #[serde(default)]
    pub files_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgressEvent {
    pub current: u64,
    pub total: u64,
    pub path: String,
    pub phase: String,
}

/// Readiness of semantic search: whether the model is installed and how much
/// of the library has been embedded so far.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStatus {
    pub model_ready: bool,
    pub embedded: i64,
    pub total: i64,
}

/// Download progress for a model file, emitted as `model-progress`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgressEvent {
    pub model_id: String,
    pub file_index: u32,
    pub file_count: u32,
    pub downloaded: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub exported: u32,
    pub missing: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecord {
    pub id: String,
    pub path: String,
    pub asset_count: i64,
    pub exported_count: i64,
    pub missing_count: i64,
    pub created_at: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub detail: Option<String>,
    pub created_at: String,
    pub undone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySnapshot {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_stack: Vec<ActivityEntry>,
    pub redo_stack: Vec<ActivityEntry>,
    pub activity: Vec<ActivityEntry>,
}

/// Sidebar badge counts for the smart collections, keyed by collection id.
/// A map rather than a struct so adding a collection is a one-line change in
/// `smart` instead of a change rippling through the IPC types and the sidebar.
pub type SmartCounts = std::collections::HashMap<String, i64>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub locked_count: i64,
    pub recovery_configured: bool,
    pub unlocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// True when at least one vault exists.
    pub configured: bool,
    pub unlocked: bool,
    pub recovery_configured: bool,
    pub vault_id: Option<String>,
    pub vault_name: Option<String>,
    pub vault_path: Option<String>,
    /// Locked items in the active vault when unlocked; otherwise total across all vaults.
    pub locked_count: i64,
    /// Sum of locked items across every vault (for sidebar badge).
    pub total_locked_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedAsset {
    pub id: String,
    pub file_name: String,
    /// Path relative to the locked group root, so folder structure survives a
    /// move back out. Equals `file_name` for individually locked items.
    pub rel_path: String,
    pub media_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub size_bytes: Option<i64>,
    pub has_thumb: bool,
    pub album_id: Option<String>,
    pub locked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedAlbum {
    pub id: String,
    pub name: String,
    pub item_count: i64,
    pub created_at: String,
}

/// Returned once, immediately after setup: the only time the recovery code is
/// ever shown. It is not persisted anywhere.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSetupResult {
    pub status: VaultStatus,
    pub recovery_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockResult {
    pub locked: usize,
    /// Set when the items were locked as a group (album or folder).
    pub album_id: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveOutResult {
    pub restored: usize,
    pub paths: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeveloperInfo {
    pub app_version: String,
    pub build_profile: String,
    pub debug_build: bool,
    pub os: String,
    pub arch: String,
    pub app_data_path: String,
    pub database_path: String,
    pub database_size_bytes: u64,
    pub schema_version: i64,
    pub thumbnails_path: String,
    pub thumbnail_count: u64,
    pub thumbnail_size_bytes: u64,
    pub logs_path: String,
    pub log_file_count: u64,
    pub log_size_bytes: u64,
    pub watched_folder_count: i64,
    pub activity_count: i64,
    pub export_count: i64,
    pub import_run_count: i64,
    pub ffmpeg_available: bool,
    pub index_progress: IndexProgress,
    pub recent_logs: Vec<String>,
    pub crash_logs: Vec<String>,
}
