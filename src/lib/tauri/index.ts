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

export type BlurryAsset = {
  asset: AssetSummary;
  blurScore: number;
};

export type ImportResult = {
  scanned: number;
  inserted: number;
  updated: number;
  skipped: number;
  cancelled?: boolean;
  durationMs?: number;
  filesPerSec?: number;
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
  | "panoramas"
  | "documents"
  | "receipts";

export const SMART_COLLECTION_KINDS: readonly SmartCollectionKind[] = [
  "videos",
  "rawPhotos",
  "screenshots",
  "selfies",
  "panoramas",
  "documents",
  "receipts",
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
  ocrReady: boolean;
  facesReady: boolean;
  tagsReady: boolean;
  models: ModelInfo[];
  semanticDownloadBytes: number;
  ocrDownloadBytes: number;
  facesDownloadBytes: number;
  tagsDownloadBytes: number;
  installedBytes: number;
  modelsDir: string;
};

export type EmbedProgress = {
  pending: number;
  embedded: number;
  total: number;
  failed: number;
  running: boolean;
  paused: boolean;
  lastPath: string | null;
  lastError: string | null;
  modelReady: boolean;
};

export type OcrProgress = {
  pending: number;
  done: number;
  total: number;
  failed: number;
  running: boolean;
  paused: boolean;
  lastPath: string | null;
  lastError: string | null;
  modelReady: boolean;
};

export type OcrStatus = {
  modelReady: boolean;
  enabled: boolean;
  done: number;
  total: number;
};

export type AssetText = {
  assetId: string;
  text: string;
  lang: string | null;
  confidence: number;
  createdAt: string;
};

export type FacesProgress = {
  pending: number;
  done: number;
  total: number;
  failed: number;
  running: boolean;
  paused: boolean;
  lastPath: string | null;
  lastError: string | null;
  modelReady: boolean;
};

export type FacesStatus = {
  modelReady: boolean;
  enabled: boolean;
  done: number;
  total: number;
  peopleCount: number;
};

export type TagsProgress = {
  pending: number;
  done: number;
  total: number;
  failed: number;
  running: boolean;
  paused: boolean;
  lastPath: string | null;
  lastError: string | null;
  modelReady: boolean;
};

export type TagsStatus = {
  modelReady: boolean;
  enabled: boolean;
  done: number;
  total: number;
};

export type AssetLabel = {
  assetId: string;
  label: string;
  score: number;
  rank: number;
  modelId: string;
  createdAt: string;
};

export type ImportRun = {
  id: string;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  scanned: number;
  inserted: number;
  updated: number;
  skipped: number;
  cancelled: boolean;
  filesPerSec: number | null;
  rootsJson: string | null;
  note: string | null;
};

export type LibraryOptionStatus = {
  id: string;
  capability: string;
  capabilityLabel: string;
  name: string;
  summary: string;
  runtime: "onnx" | "native" | string;
  license: string;
  bundle: string | null;
  downloadBytes: number;
  installed: boolean;
  active: boolean;
  available: boolean;
  inputSize: number | null;
};


export type Person = {
  id: string;
  name: string | null;
  faceCount: number;
  coverCropPath: string | null;
  createdAt: string;
  ignored: boolean;
};

/** A reverse-geocoded location grouping the photos taken there. */
export type PlaceGroup = {
  label: string;
  country: string | null;
  assetCount: number;
  coverThumbnailPath: string | null;
  lat: number;
  lon: number;
};

export type PlacesProgress = {
  pending: number;
  done: number;
  total: number;
  running: boolean;
  lastPath: string | null;
};

export type FaceBox = {
  id: string;
  assetId: string;
  personId: string | null;
  personName: string | null;
  bboxX: number;
  bboxY: number;
  bboxW: number;
  bboxH: number;
  score: number;
  cropPath: string | null;
  personIgnored: boolean;
};

export type SemanticStatus = {
  modelReady: boolean;
  embedded: number;
  total: number;
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
  importRunCount: number;
  ffmpegAvailable: boolean;
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
    ignorePatterns: string[];
  };
  ai: {
    semanticSearch: boolean;
    faceRecognition: boolean;
    objectDetection: boolean;
    ocr: boolean;
    autoAlbums: boolean;
    processingDevice: string;
    backgroundProcessing: string;
    semanticModel?: string;
    ocrModel?: string;
    facesModel?: string;
    tagsModel?: string;
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
    exportMaxEdge: number;
    exportNaming: string;
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

export type SavedEditOps = {
  assetId: string;
  revisionId: string;
  ops: EditOps;
  createdAt: string;
};

export type EditRevisionSummary = {
  id: string;
  createdAt: string;
};

/** True when ops leave pixels unchanged (UI “edited” badge). */
export function isIdentityEditOps(ops: EditOps): boolean {
  const rotate = ((ops.rotateDegrees % 360) + 360) % 360;
  const crop = ops.crop;
  const cropFull =
    !crop ||
    (crop.x <= 0.001 &&
      crop.y <= 0.001 &&
      crop.width >= 0.999 &&
      crop.height >= 0.999);
  return (
    rotate === 0 &&
    !ops.flipHorizontal &&
    !ops.flipVertical &&
    Math.abs(ops.exposure) <= 0.001 &&
    cropFull
  );
}

export const api = {
  getLibraryStats: () => invoke<LibraryStats>("get_library_stats"),
  importFolder: (path: string) => invoke<ImportResult>("import_folder", { path }),
  importPaths: (paths: string[]) => invoke<ImportResult>("import_paths", { paths }),
  cancelImport: () => invoke<void>("cancel_import"),
  listAssets: (limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_assets", { limit, offset }),
  searchAssets: (query: string, limit: number, offset: number) =>
    invoke<AssetSummary[]>("search_assets", { query, limit, offset }),
  getIndexProgress: () => invoke<IndexProgress>("get_index_progress"),
  getDeveloperInfo: () => invoke<DeveloperInfo>("get_developer_info"),
  getPreferences: () => invoke<Preferences>("get_preferences"),
  setPreferences: (prefs: Preferences) =>
    invoke<Preferences>("set_preferences", { prefs }),
  pingUserActivity: () => invoke<void>("ping_user_activity"),
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
  deleteAlbum: (id: string, deleteAssets = false) =>
    invoke<number>("delete_album", {
      id,
      deleteAssets,
      delete_assets: deleteAssets,
    }),
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
  pauseEmbedding: () => invoke<void>("pause_embedding"),
  semanticSearch: (query: string, limit: number) =>
    invoke<AssetSummary[]>("semantic_search", { query, limit }),
  installOcrModels: () => invoke<MlStatus>("install_ocr_models"),
  ocrStatus: () => invoke<OcrStatus>("ocr_status"),
  ocrProgress: () => invoke<OcrProgress>("ocr_progress"),
  kickOcr: () => invoke<void>("kick_ocr"),
  pauseOcr: () => invoke<void>("pause_ocr"),
  clearOcrText: () => invoke<number>("clear_ocr_text"),
  getAssetText: (assetId: string) =>
    invoke<AssetText | null>("get_asset_text", {
      assetId,
      asset_id: assetId,
    }),
  installFaceModels: () => invoke<MlStatus>("install_face_models"),
  facesStatus: () => invoke<FacesStatus>("faces_status"),
  facesProgress: () => invoke<FacesProgress>("faces_progress"),
  kickFaces: () => invoke<void>("kick_faces"),
  pauseFaces: () => invoke<void>("pause_faces"),
  clearFaceData: () => invoke<number>("clear_face_data"),
  installTagsModels: () => invoke<MlStatus>("install_tags_models"),
  tagsStatus: () => invoke<TagsStatus>("tags_status"),
  tagsProgress: () => invoke<TagsProgress>("tags_progress"),
  kickTags: () => invoke<void>("kick_tags"),
  pauseTags: () => invoke<void>("pause_tags"),
  clearAutoTags: () => invoke<number>("clear_auto_tags"),
  listAssetLabels: (assetId: string) =>
    invoke<AssetLabel[]>("list_asset_labels", {
      assetId,
      asset_id: assetId,
    }),
  listImportRuns: (limit = 20) =>
    invoke<ImportRun[]>("list_import_runs", { limit }),
  modelLibrary: () => invoke<LibraryOptionStatus[]>("model_library"),
  installModelOption: (optionId: string) =>
    invoke<MlStatus>("install_model_option", {
      optionId,
      option_id: optionId,
    }),
  setActiveModel: (optionId: string, reprocess = true) =>
    invoke<LibraryOptionStatus[]>("set_active_model", {
      optionId,
      option_id: optionId,
      reprocess,
    }),
  listPeople: () => invoke<Person[]>("list_people"),
  listIgnoredPeople: () => invoke<Person[]>("list_ignored_people"),
  setPersonIgnored: (personId: string, ignored: boolean) =>
    invoke<void>("set_person_ignored", {
      personId,
      person_id: personId,
      ignored,
    }),
  listPersonAssets: (personId: string, limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_person_assets", {
      personId,
      person_id: personId,
      limit,
      offset,
    }),
  renamePerson: (personId: string, name: string) =>
    invoke<void>("rename_person", {
      personId,
      person_id: personId,
      name,
    }),
  mergePeople: (intoId: string, fromId: string) =>
    invoke<void>("merge_people", {
      intoId,
      into_id: intoId,
      fromId,
      from_id: fromId,
    }),
  detachFace: (faceId: string) =>
    invoke<string>("detach_face", { faceId, face_id: faceId }),
  listAssetFaces: (assetId: string) =>
    invoke<FaceBox[]>("list_asset_faces", {
      assetId,
      asset_id: assetId,
    }),
  reclusterFaces: () => invoke<number>("recluster_faces"),
  reprocessAi: (kinds: Array<"semantic" | "ocr" | "faces" | "tags" | "all">) =>
    invoke<{
      embeddingsCleared: number;
      ocrCleared: number;
      facesCleared: number;
      tagsCleared: number;
    }>("reprocess_ai", { kinds }),
  listPlaces: () => invoke<PlaceGroup[]>("list_places"),
  listPlaceAssets: (label: string, limit: number, offset: number) =>
    invoke<AssetSummary[]>("list_place_assets", { label, limit, offset }),
  placesProgress: () => invoke<PlacesProgress>("places_progress"),
  kickPlaces: () => invoke<void>("kick_places"),
  clearPlaces: () => invoke<number>("clear_places"),
  findDuplicates: () => invoke<DuplicateGroup[]>("find_duplicates"),
  listBlurryAssets: (limit = 200, offset = 0) =>
    invoke<BlurryAsset[]>("list_blurry_assets", { limit, offset }),
  scanBlurScores: (limit = 500) =>
    invoke<number>("scan_blur_scores", { limit }),
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
  saveEditOps: (assetId: string, ops: EditOps) =>
    invoke<SavedEditOps>("save_edit_ops", {
      assetId,
      asset_id: assetId,
      ops,
    }),
  getEditOps: (assetId: string) =>
    invoke<SavedEditOps | null>("get_edit_ops", {
      assetId,
      asset_id: assetId,
    }),
  listEditRevisions: (assetId: string) =>
    invoke<EditRevisionSummary[]>("list_edit_revisions", {
      assetId,
      asset_id: assetId,
    }),
  revertEditRevision: (assetId: string, revisionId: string) =>
    invoke<SavedEditOps>("revert_edit_revision", {
      assetId,
      asset_id: assetId,
      revisionId,
      revision_id: revisionId,
    }),
  clearEditOps: (assetId: string) =>
    invoke<void>("clear_edit_ops", {
      assetId,
      asset_id: assetId,
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

export function fileSrc(path: string | null | undefined): string | null {
  if (!path) return null;
  try {
    return convertFileSrc(path);
  } catch {
    return null;
  }
}
