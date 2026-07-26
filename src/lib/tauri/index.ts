import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";

export type AssetSummary = {
  id: string;
  path: string;
  hash: string;
  perceptualHash?: string | null;
  mediaType: string;
  width?: number | null;
  height?: number | null;
  durationMs?: number | null;
  createdAt: string;
  capturedAt?: string | null;
  indexedAt: string;
  favorite: boolean;
  rating: number;
  colorLabel?: string | null;
  thumbnailPath?: string | null;
  camera?: string | null;
  lens?: string | null;
  deletedAt?: string | null;
};

export type LibraryStats = {
  totalAssets: number;
  totalImages: number;
  totalVideos: number;
  favorites: number;
  inTrash: number;
  albumCount: number;
  tagCount: number;
  watchedFolders: number;
  trashRetentionDays: number;
};

export type Album = {
  id: string;
  name: string;
  coverAssetId?: string | null;
  coverThumbnailPath?: string | null;
  createdAt: string;
  assetCount: number;
};

export type Tag = {
  id: string;
  name: string;
  assetCount: number;
};

export type SavedSearch = {
  id: string;
  name: string;
  query: string;
  createdAt: string;
  updatedAt: string;
};

export type AssetOrganisation = {
  albums: Album[];
  tags: Tag[];
};

export type FacetCount = {
  value: string;
  count: number;
};

export type LibraryFacets = {
  ratings: FacetCount[];
  colorLabels: FacetCount[];
};

export type TagBrowseFilter = {
  tagIds: string[];
  ratings: number[];
  colorLabels: string[];
};

export type TimelineMonth = {
  year: number;
  month: number;
  count: number;
};

export type IndexProgress = {
  pending: number;
  processed: number;
  running: boolean;
  lastPath?: string | null;
};

export type DuplicateGroup = {
  kind: string;
  key: string;
  assetIds: string[];
};

export type ImportResult = {
  scanned: number;
  inserted: number;
  updated: number;
  skipped: number;
};

export type ImportProgressEvent = {
  current: number;
  total: number;
  path: string;
  phase: string;
};

export type PermanentDeleteResult = {
  removedFromLibrary: number;
  filesDeleted: number;
  thumbsDeleted: number;
  errors: string[];
};

export type ExportResult = {
  path: string;
  exported: number;
  missing: number;
  errors: string[];
};

export type ExportRecord = {
  id: string;
  path: string;
  assetCount: number;
  exportedCount: number;
  missingCount: number;
  createdAt: string;
  note?: string | null;
};

export type ActivityEntry = {
  id: string;
  kind: string;
  label: string;
  detail?: string | null;
  createdAt: string;
  undone: boolean;
};

export type HistorySnapshot = {
  canUndo: boolean;
  canRedo: boolean;
  undoStack: ActivityEntry[];
  redoStack: ActivityEntry[];
  activity: ActivityEntry[];
};

/** Standing library filters surfaced as their own sidebar destinations. */
export type SmartCollectionKind =
  | "videos"
  | "rawPhotos"
  | "screenshots"
  | "selfies"
  | "panoramas";

export const SMART_COLLECTION_KINDS: readonly SmartCollectionKind[] = [
  "videos",
  "rawPhotos",
  "screenshots",
  "selfies",
  "panoramas",
];

/** Counts keyed by collection id; the backend always returns every key. */
export type SmartCounts = Record<SmartCollectionKind, number>;

export function isSmartCollectionKind(
  value: string,
): value is SmartCollectionKind {
  return (SMART_COLLECTION_KINDS as readonly string[]).includes(value);
}

/** A model LUMORA knows how to install, and whether it is installed. */
export type ModelInfo = {
  id: string;
  bundle: string;
  kind: string;
  version: string;
  fileName: string;
  sizeBytes: number;
  license: string;
  installed: boolean;
  path: string | null;
  installedAt: string | null;
};

export type MlStatus = {
  semanticReady: boolean;
  models: ModelInfo[];
  semanticDownloadBytes: number;
  installedBytes: number;
  modelsDir: string;
};

export type SemanticStatus = {
  modelReady: boolean;
  embedded: number;
  total: number;
};

export type EmbedProgress = {
  pending: number;
  embedded: number;
  total: number;
  running: boolean;
  lastPath: string | null;
  modelReady: boolean;
};

export type ModelProgressEvent = {
  modelId: string;
  fileIndex: number;
  fileCount: number;
  downloaded: number;
  total: number;
};

export type VaultSummary = {
  id: string;
  name: string;
  path: string;
  lockedCount: number;
  recoveryConfigured: boolean;
  unlocked: boolean;
};

export type VaultStatus = {
  configured: boolean;
  unlocked: boolean;
  recoveryConfigured: boolean;
  vaultId?: string | null;
  vaultName?: string | null;
  vaultPath?: string | null;
  lockedCount: number;
  totalLockedCount: number;
};

export type LockedAsset = {
  id: string;
  fileName: string;
  /** Path within its locked folder; equals fileName for loose items. */
  relPath: string;
  albumId?: string | null;
  mediaType: string;
  width?: number | null;
  height?: number | null;
  sizeBytes?: number | null;
  hasThumb: boolean;
  lockedAt: string;
};

/** An album or folder moved into the vault as a unit. */
export type LockedAlbum = {
  id: string;
  name: string;
  itemCount: number;
  createdAt: string;
};

export type LockResult = {
  locked: number;
  albumId?: string | null;
  errors: string[];
};

/** Returned once, right after setup — the only time the recovery code exists. */
export type VaultSetupResult = {
  status: VaultStatus;
  recoveryCode: string;
};

export type MoveOutResult = {
  restored: number;
  paths: string[];
  errors: string[];
};

export type DeveloperInfo = {
  appVersion: string;
  buildProfile: string;
  debugBuild: boolean;
  os: string;
  arch: string;
  appDataPath: string;
  databasePath: string;
  databaseSizeBytes: number;
  schemaVersion: number;
  thumbnailsPath: string;
  thumbnailCount: number;
  thumbnailSizeBytes: number;
  logsPath: string;
  logFileCount: number;
  logSizeBytes: number;
  watchedFolderCount: number;
  activityCount: number;
  exportCount: number;
  indexProgress: IndexProgress;
  recentLogs: string[];
  crashLogs: string[];
};

export type Preferences = {
  general: {
    restorePreviousSession: boolean;
    doubleClickOpensViewer: boolean;
    confirmBeforeDeleting: boolean;
    revealImportedPhotos: boolean;
    language: string;
    dateFormat: string;
  };
  appearance: {
    theme: string;
    accent: string;
    thumbnailSize: number;
    density: string;
    animations: boolean;
    smoothScrolling: boolean;
  };
  library: {
    watchFoldersEnabled: boolean;
    autoScan: string;
  };
  ai: {
    semanticSearch: boolean;
    faceRecognition: boolean;
    objectDetection: boolean;
    ocr: boolean;
    autoAlbums: boolean;
    processingDevice: string;
    backgroundProcessing: string;
  };
  privacy: {
    autoLockMinutes: number;
    preserveGps: boolean;
    preserveExif: boolean;
    stripMetadataOnExport: boolean;
  };
  performance: {
    cpuProfile: string;
    pauseOnBattery: boolean;
    thumbnailCacheMb: number;
  };
  importExport: {
    skipDuplicates: boolean;
    preserveFolderStructure: boolean;
    jpegQuality: number;
    stripMetadata: boolean;
  };
  updates: {
    checkAutomatically: boolean;
    downloadInBackground: boolean;
  };
};

export type StorageSummary = {
  databaseBytes: number;
  thumbnailBytes: number;
  thumbnailCount: number;
  modelsBytes: number;
  embeddingsBytes: number;
  logsBytes: number;
  appDataPath: string;
  thumbsPath: string;
  modelsPath: string;
  logsPath: string;
  databasePath: string;
};

export type CropRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type EditOps = {
  rotateDegrees: number;
  flipHorizontal?: boolean;
  flipVertical?: boolean;
  crop?: CropRect | null;
  exposure: number;
};

export type EditSaveMode = "replace" | "copy";

export type EditResult = {
  asset: AssetSummary;
  mode: EditSaveMode;
  embeddingQueued: boolean;
};

export const api = {
  getLibraryStats: () => invoke<LibraryStats>("get_library_stats"),
  importFolder: (path: string) => invoke<ImportResult>("import_folder", { path }),
  importPaths: (paths: string[]) => invoke<ImportResult>("import_paths", { paths }),
  listAssets: (limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_assets", { limit, offset }),
  searchAssets: (query: string, limit: number, offset: number) =>
    invoke<AssetSummary[]>("search_assets", { query, limit, offset }),
  getIndexProgress: () => invoke<IndexProgress>("get_index_progress"),
  getDeveloperInfo: () => invoke<DeveloperInfo>("get_developer_info"),
  getPreferences: () => invoke<Preferences>("get_preferences"),
  setPreferences: (prefs: Preferences) =>
    invoke<Preferences>("set_preferences", { prefs }),
  getStorageSummary: () => invoke<StorageSummary>("get_storage_summary"),
  clearThumbnailCache: () => invoke<number>("clear_thumbnail_cache"),
  rebuildThumbnailCache: () => invoke<number>("rebuild_thumbnail_cache"),
  optimizeDatabase: () => invoke<void>("optimize_database"),
  setFavorite: (id: string, favorite: boolean) =>
    invoke<void>("set_favorite", { id, favorite }),
  setFavorites: (ids: string[], favorite: boolean) =>
    invoke<number>("set_favorites", { ids, favorite }),
  setRating: (id: string, rating: number) => invoke<void>("set_rating", { id, rating }),
  setRatings: (ids: string[], rating: number) =>
    invoke<number>("set_ratings", { ids, rating }),
  setColorLabel: (id: string, colorLabel: string | null) =>
    invoke<void>("set_color_label", { id, colorLabel }),
  setColorLabels: (ids: string[], colorLabel: string | null) =>
    invoke<number>("set_color_labels", { ids, colorLabel }),
  listTags: () => invoke<Tag[]>("list_tags"),
  listTagAssets: (tagId: string, limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_tag_assets", { tagId, limit, offset }),
  getLibraryFacets: () => invoke<LibraryFacets>("get_library_facets"),
  listTagBrowseAssets: (
    filter: TagBrowseFilter,
    limit: number,
    offset: number,
  ) =>
    invoke<AssetSummary[]>("list_tag_browse_assets", { filter, limit, offset }),
  createTag: (name: string) => invoke<Tag>("create_tag", { name }),
  tagAsset: (assetId: string, tagId: string) =>
    invoke<void>("tag_asset", { assetId, tagId }),
  tagAssets: (tagId: string, assetIds: string[]) =>
    invoke<number>("tag_assets", { tagId, assetIds }),
  createTagAndAssign: (name: string, assetIds: string[]) =>
    invoke<Tag>("create_tag_and_assign", { name, assetIds }),
  untagAsset: (assetId: string, tagId: string) =>
    invoke<void>("untag_asset", { assetId, tagId }),
  listAlbums: () => invoke<Album[]>("list_albums"),
  listSavedSearches: () => invoke<SavedSearch[]>("list_saved_searches"),
  recordRecentSearch: (query: string) =>
    invoke<SavedSearch>("record_recent_search", { query }),
  deleteSavedSearch: (id: string) => invoke<void>("delete_saved_search", { id }),
  clearRecentSearches: () => invoke<number>("clear_recent_searches"),
  getAssetOrganisation: (id: string) =>
    invoke<AssetOrganisation>("get_asset_organisation", { id }),
  createAlbum: (name: string) => invoke<Album>("create_album", { name }),
  createAlbumWithAssets: (name: string, assetIds: string[]) =>
    invoke<Album>("create_album_with_assets", { name, assetIds, asset_ids: assetIds }),
  renameAlbum: (id: string, name: string) => invoke<void>("rename_album", { id, name }),
  deleteAlbum: (id: string) => invoke<void>("delete_album", { id }),
  addToAlbum: (albumId: string, assetId: string) =>
    invoke<void>("add_to_album", {
      albumId,
      assetId,
      album_id: albumId,
      asset_id: assetId,
    }),
  addAssetsToAlbum: (albumId: string, assetIds: string[]) =>
    invoke<number>("add_assets_to_album", {
      albumId,
      assetIds,
      album_id: albumId,
      asset_ids: assetIds,
    }),
  removeFromAlbum: (albumId: string, assetId: string) =>
    invoke<void>("remove_from_album", {
      albumId,
      assetId,
      album_id: albumId,
      asset_id: assetId,
    }),
  listAlbumAssets: (albumId: string, limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_album_assets", {
      albumId,
      album_id: albumId,
      limit,
      offset,
    }),
  timelineMonths: () => invoke<TimelineMonth[]>("timeline_months"),
  listAssetsForMonth: (year: number, month: number, limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_assets_for_month", { year, month, limit, offset }),
  listRecent: (limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_recent", { limit, offset }),
  listRecentlyViewed: (limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_recently_viewed", { limit, offset }),
  recordAssetView: (id: string) => invoke<void>("record_asset_view", { id }),
  listSmartCollection: (kind: SmartCollectionKind, limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_smart_collection", { kind, limit, offset }),
  smartCollectionCounts: () => invoke<SmartCounts>("smart_collection_counts"),
  mlStatus: () => invoke<MlStatus>("ml_status"),
  /** Explicit, user-initiated download — the only network call in the app. */
  installSemanticModels: () => invoke<MlStatus>("install_semantic_models"),
  removeMlModel: (id: string) => invoke<MlStatus>("remove_ml_model", { id }),
  clearMlEmbeddings: () => invoke<number>("clear_ml_embeddings"),
  semanticStatus: () => invoke<SemanticStatus>("semantic_status"),
  embedProgress: () => invoke<EmbedProgress>("embed_progress"),
  kickEmbedding: () => invoke<void>("kick_embedding"),
  semanticSearch: (query: string, limit: number) =>
    invoke<AssetSummary[]>("semantic_search", { query, limit }),
  findDuplicates: () => invoke<DuplicateGroup[]>("find_duplicates"),
  listAssetsByIds: (ids: string[]) =>
    invoke<AssetSummary[]>("list_assets_by_ids", { ids }),
  softDeleteAssets: (ids: string[]) => invoke<number>("soft_delete_assets", { ids }),
  restoreAssets: (ids: string[]) => invoke<number>("restore_assets", { ids }),
  listTrash: (limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_trash", { limit, offset }),
  purgeTrash: () => invoke<number>("purge_trash"),
  emptyTrash: () => invoke<PermanentDeleteResult>("empty_trash"),
  permanentlyDeleteAssets: (ids: string[], deleteFiles: boolean) =>
    invoke<PermanentDeleteResult>("permanently_delete_assets", {
      ids,
      deleteFiles,
      delete_files: deleteFiles,
    }),
  exportAssetsZip: (ids: string[], dest: string) =>
    invoke<ExportResult>("export_assets_zip", { ids, dest }),
  applyImageEdit: (assetId: string, ops: EditOps, mode: EditSaveMode) =>
    invoke<EditResult>("apply_image_edit", {
      assetId,
      asset_id: assetId,
      ops,
      mode,
    }),
  getHistory: () => invoke<HistorySnapshot>("get_history"),
  listExports: (limit = 50) => invoke<ExportRecord[]>("list_exports", { limit }),
  undoLast: () => invoke<boolean>("undo_last"),
  redoLast: () => invoke<boolean>("redo_last"),
  listWatchedFolders: () => invoke<string[]>("list_watched_folders"),
  removeWatchedFolder: (path: string) =>
    invoke<boolean>("remove_watched_folder", { path }),
  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  listVaults: () => invoke<VaultSummary[]>("list_vaults"),
  setupVault: (name: string, vaultPath: string, password: string) =>
    invoke<VaultSetupResult>("setup_vault", {
      name,
      vaultPath,
      vault_path: vaultPath,
      password,
    }),
  unlockVault: (vaultId: string, password: string) =>
    invoke<VaultStatus>("unlock_vault", {
      vaultId,
      vault_id: vaultId,
      password,
    }),
  recoverVault: (vaultId: string, recoveryCode: string, newPassword: string) =>
    invoke<VaultStatus>("recover_vault", {
      vaultId,
      vault_id: vaultId,
      recoveryCode,
      recovery_code: recoveryCode,
      newPassword,
      new_password: newPassword,
    }),
  enableVaultRecovery: () => invoke<string>("enable_vault_recovery"),
  lockVault: () => invoke<VaultStatus>("lock_vault"),
  lockAssetsToVault: (ids: string[], vaultId: string) =>
    invoke<LockResult>("lock_assets_to_vault", {
      ids,
      vaultId,
      vault_id: vaultId,
    }),
  lockAlbumToVault: (albumId: string, vaultId: string) =>
    invoke<LockResult>("lock_album_to_vault", {
      albumId,
      album_id: albumId,
      vaultId,
      vault_id: vaultId,
    }),
  lockFolderToVault: (path: string, vaultId: string) =>
    invoke<LockResult>("lock_folder_to_vault", {
      path,
      vaultId,
      vault_id: vaultId,
    }),
  listLockedAlbums: () => invoke<LockedAlbum[]>("list_locked_albums"),
  /** Omit albumId for the loose items that aren't inside a locked folder. */
  listLockedAssets: (albumId?: string | null) =>
    invoke<LockedAsset[]>("list_locked_assets", {
      albumId: albumId ?? null,
      album_id: albumId ?? null,
    }),
  vaultThumb: (id: string) => invoke<string | null>("vault_thumb", { id }),
  vaultMedia: (id: string) => invoke<string>("vault_media", { id }),
  moveOutLockedAssets: (ids: string[], dest: string) =>
    invoke<MoveOutResult>("move_out_locked_assets", { ids, dest }),
  moveOutLockedAlbum: (albumId: string, dest: string) =>
    invoke<MoveOutResult>("move_out_locked_album", {
      albumId,
      album_id: albumId,
      dest,
    }),
  deleteLockedAssets: (ids: string[]) =>
    invoke<number>("delete_locked_assets", { ids }),
  deleteLockedAlbum: (albumId: string) =>
    invoke<number>("delete_locked_album", {
      albumId,
      album_id: albumId,
    }),
};

export function thumbSrc(asset: AssetSummary): string | null {
  if (!asset.thumbnailPath) return null;
  try {
    return convertFileSrc(asset.thumbnailPath);
  } catch {
    return null;
  }
}

export function fileSrc(path: string): string {
  return convertFileSrc(path);
}
