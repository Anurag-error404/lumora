import { useMemo, useState, useEffect, useRef } from "react";
import { Icon } from "./components/icons";
import { PageHeader } from "./components/PageHeader";
import { AlbumPickBar } from "./components/layout/AlbumPickBar";
import { DropDock } from "./components/layout/DropDock";
import { ImportProgressBar } from "./components/layout/ImportProgressBar";
import { SelectionBar } from "./components/layout/SelectionBar";
import { Sidebar } from "./components/layout/Sidebar";
import { StatusBar } from "./components/layout/StatusBar";
import { Toolbar } from "./components/layout/Toolbar";
import { AlbumModal } from "./components/modals/AlbumModal";
import { DeleteAlbumDialog } from "./components/modals/DeleteAlbumDialog";
import { ImportModal } from "./components/modals/ImportModal";
import { PersonModal } from "./components/modals/PersonModal";
import { TagModal } from "./components/modals/TagModal";
import { VaultPickerDialog } from "./components/modals/VaultPickerDialog";
import { ActivityView } from "./features/activity/ActivityView";
import { AlbumDetailHeader } from "./features/albums/AlbumDetailHeader";
import { AlbumsGridView } from "./features/albums/AlbumsGridView";
import { DeveloperView } from "./features/developer/DeveloperView";
import { DuplicatesView } from "./features/duplicates/DuplicatesView";
import { ExportsView } from "./features/exports/ExportsView";
import { SettingsView } from "./features/settings/SettingsView";
import { RecentSearchesView } from "./features/search/RecentSearchesView";
import { AssetEmptyState } from "./features/library/AssetEmptyState";
import { LibraryGrid } from "./features/library/LibraryGrid";
import { MediaInfoPanel } from "./features/media-info/MediaInfoPanel";
import { PeopleView } from "./features/people/PeopleView";
import { PlacesView } from "./features/places/PlacesView";
import { MemoriesView } from "./features/memories/MemoriesView";
import { TagFilterBoard } from "./features/tags/TagFilterBoard";
import { TimelineView } from "./features/timeline/TimelineView";
import { HomeView } from "./features/home/HomeView";
import { LockedFolderView } from "./features/vault/LockedFolderView";
import { MediaViewer } from "./features/viewer/MediaViewer";
import { WatchedFoldersView } from "./features/watched/WatchedFoldersView";
import { useAlbums } from "./hooks/useAlbums";
import { useAlbumWorkflows } from "./hooks/useAlbumWorkflows";
import { useAssetActions } from "./hooks/useAssetActions";
import { useDeveloperInfo } from "./hooks/useDeveloperInfo";
import { useDuplicates } from "./hooks/useDuplicates";
import { useExportsFeed } from "./hooks/useExportsFeed";
import { useHistoryFeed } from "./hooks/useHistoryFeed";
import { useImportFlow } from "./hooks/useImportFlow";
import { useImportProgress } from "./hooks/useImportProgress";
import { useIndexProgress } from "./hooks/useIndexProgress";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { useLibraryAssets } from "./hooks/useLibraryAssets";
import { useMarqueeSelection } from "./hooks/useMarqueeSelection";
import { useMemories } from "./hooks/useMemories";
import { usePeople } from "./hooks/usePeople";
import { usePlaces } from "./hooks/usePlaces";
import { usePreferences } from "./hooks/usePreferences";
import { useAppUpdater } from "./hooks/useAppUpdater";
import { useRecentSearches } from "./hooks/useRecentSearches";
import { useTagBrowse } from "./hooks/useTagBrowse";
import { useTagWorkflows } from "./hooks/useTagWorkflows";
import { useTimeline } from "./hooks/useTimeline";
import { useVault } from "./hooks/useVault";
import { useVaultAutoLock } from "./hooks/useVaultAutoLock";
import { useViewer } from "./hooks/useViewer";
import { useWatchedFolders } from "./hooks/useWatchedFolders";
import { api, type Album, type AssetSummary, type Person } from "./lib/tauri";
import { LIBRARY_PAGE_META } from "./lib/pageMeta";
import type { AlbumPickTarget, View } from "./types/app";
import "./styles/app.css";

export default function App() {
  const [view, setView] = useState<View>("home");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [homeRecent, setHomeRecent] = useState<AssetSummary[]>([]);
  const [pickingForAlbum, setPickingForAlbum] = useState<AlbumPickTarget | null>(
    null,
  );
  const [vaultPick, setVaultPick] = useState<
    | { kind: "assets"; ids: string[] }
    | { kind: "album"; albumId: string; albumName: string }
    | null
  >(null);
  const [deleteAlbumTarget, setDeleteAlbumTarget] = useState<Album | null>(null);
  const [personModal, setPersonModal] = useState<Person | null>(null);
  const [personName, setPersonName] = useState("");
  const lastActivityPing = useRef(0);

  useEffect(() => {
    const ping = () => {
      const now = Date.now();
      if (now - lastActivityPing.current < 15_000) return;
      lastActivityPing.current = now;
      void api.pingUserActivity().catch(() => undefined);
    };
    window.addEventListener("pointerdown", ping);
    window.addEventListener("keydown", ping);
    window.addEventListener("mousemove", ping);
    return () => {
      window.removeEventListener("pointerdown", ping);
      window.removeEventListener("keydown", ping);
      window.removeEventListener("mousemove", ping);
    };
  }, []);

  const { albums, activeAlbum, setActiveAlbum, refreshAlbums } = useAlbums({
    view,
    setError,
  });

  const {
    people,
    ignoredPeople,
    activePerson,
    setActivePerson,
    refreshPeople,
    setPersonIgnored,
  } = usePeople({
    view,
    setError,
  });

  const { places, activePlace, setActivePlace, refreshPlaces } = usePlaces({
    view,
    setError,
  });

  const {
    memories,
    activeMemory,
    setActiveMemory,
    openMemoryDetail,
    refreshMemories,
    saveAsAlbum,
    saving: savingMemoryAlbum,
  } = useMemories({
    view,
    setError,
  });

  const {
    tags,
    tagBrowse,
    tagBrowseActive,
    ratingCounts,
    colorCounts,
    tagBrowseSummary,
    refreshTags,
    toggleTagFilter,
    toggleRatingFilter,
    toggleColorFilter,
    clearTagBrowse,
  } = useTagBrowse({ view, setError, setSelected });

  const {
    timeline,
    timelineAssets,
    setTimelineAssets,
    timelineVisibleCount,
    timelineLoading,
    timelineSentinelRef,
    timelineKey,
    visibleTimelineMonths,
    timelineYears,
    timelineScaleYears,
    visibleTimelineAssetCount,
    timelineFlatAssets,
    jumpToYear,
    selectTimelineGroup,
  } = useTimeline({ view, setError, selected, setSelected });

  const { dupes, dupeAssets, setDupeAssets, blurry, loadDuplicates, dupeAssetList } =
    useDuplicates({ view, setError });

  const { history, refreshHistory } = useHistoryFeed({ view, setError });
  const { exports, refreshExports } = useExportsFeed({ view, setError });
  const { developerInfo, developerLoading, refreshDeveloperInfo } =
    useDeveloperInfo({ view, setError });
  const {
    prefs,
    loading: prefsLoading,
    update: updatePrefs,
  } = usePreferences({ setError });
  const updater = useAppUpdater(prefs);
  const {
    recentSearches,
    record: recordRecentSearch,
    remove: removeRecentSearch,
    clear: clearRecentSearches,
  } = useRecentSearches({ view, setError });

  const {
    assets,
    setAssets,
    stats,
    smartCounts,
    refreshStats,
    hasMore,
    loadAssets,
    loadMoreAssets,
  } = useLibraryAssets({
    view,
    query,
    activeAlbum,
    activePerson,
    activePlace,
    activeMemory,
    tagBrowse,
    refreshHistory,
    refreshExports,
    refreshDeveloperInfo,
    setTimelineAssets,
    setError,
    semanticSearchEnabled: prefs?.ai.semanticSearch ?? true,
  });

  const {
    folders: watchedFolders,
    loading: watchedLoading,
    refresh: refreshWatched,
    addFolder: addWatchedFolder,
    removeFolder: removeWatchedFolder,
  } = useWatchedFolders({
    view,
    setError,
    setBusy,
    onImported: loadAssets,
  });

  const vault = useVault({ view, setError });
  useVaultAutoLock({
    unlocked: Boolean(vault.status?.unlocked),
    autoLockMinutes: prefs?.privacy.autoLockMinutes ?? 0,
    onLock: vault.lock,
  });

  const progress = useIndexProgress();
  const { importProgress, setImportProgress, importPct } = useImportProgress();

  const {
    lightboxId,
    setLightboxId,
    infoAssetId,
    setInfoAssetId,
    lightboxAsset,
    infoAsset,
    viewerList,
    viewerIndex,
    showPrevMedia,
    showNextMedia,
  } = useViewer({
    assets,
    timelineAssets,
    timelineFlatAssets,
    dupeAssets,
    dupeAssetList,
    homeRecent,
    loadMoreAssets,
  });

  const selectedIds = useMemo(() => [...selected], [selected]);
  const hasSelection = selectedIds.length > 0;

  function toggleSelection(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function clearSelection() {
    setSelected(new Set());
  }

  async function submitSearch() {
    const q = query.trim();
    if (view === "home" || view === "savedSearches") setView("library");
    if (q) {
      try {
        await recordRecentSearch(q);
      } catch (e) {
        setError(String(e));
      }
    }
    await loadAssets();
  }

  // Live search already reloads on query change; debounce a recent-history
  // write so typing doesn't spam the DB, but settled queries are remembered.
  useEffect(() => {
    const q = query.trim();
    if (!q) return;
    const id = window.setTimeout(() => {
      void recordRecentSearch(q).catch(() => undefined);
    }, 750);
    return () => window.clearTimeout(id);
  }, [query, recordRecentSearch]);

  function runRecentSearch(search: { query: string } | string) {
    const q = typeof search === "string" ? search : search.query;
    setQuery(q);
    setView("library");
    setSelected(new Set());
    void recordRecentSearch(q).catch((e) => setError(String(e)));
  }

  function selectAllVisible() {
    if (view === "timeline") {
      setSelected(
        new Set(
          visibleTimelineMonths.flatMap(
            (month) =>
              timelineAssets[timelineKey(month)]?.map((asset) => asset.id) ?? [],
          ),
        ),
      );
      return;
    }
    setSelected(new Set(assets.map((a) => a.id)));
  }

  async function moveSelectionToLocked() {
    if (!selectedIds.length) return;
    if (!vault.status?.configured || vault.vaults.length === 0) {
      setView("locked");
      vault.startCreate();
      setError("Create a vault first, then move items in.");
      return;
    }
    setVaultPick({ kind: "assets", ids: selectedIds });
  }

  function createLockedVault() {
    setView("locked");
    vault.startCreate();
  }

  async function moveAlbumToLocked(albumId: string, albumName: string) {
    if (!vault.status?.configured || vault.vaults.length === 0) {
      setError("Create a locked vault first, then add this album to it.");
      return;
    }
    if (
      !window.confirm(
        `Move “${albumName}” and all of its items into an encrypted vault?\n\nThe original media files will be removed.`,
      )
    ) {
      return;
    }
    setVaultPick({ kind: "album", albumId, albumName });
  }

  async function performDeleteAlbum(album: Album, deleteAssets: boolean) {
    setBusy(true);
    try {
      const trashed = await api.deleteAlbum(album.id, deleteAssets);
      setDeleteAlbumTarget(null);
      setActiveAlbum(null);
      setSelected(new Set());
      await Promise.all([refreshAlbums(), loadAssets(), refreshStats()]);
      void refreshHistory();
      setError(
        deleteAssets
          ? `Deleted “${album.name}” and moved ${trashed} item(s) to trash`
          : `Deleted album “${album.name}” · photos kept in library`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function requestDeleteAlbum(album: Album) {
    if (album.assetCount === 0) {
      if (
        window.confirm(
          `Delete empty album “${album.name}”?\n\nThis cannot be undone.`,
        )
      ) {
        void performDeleteAlbum(album, false);
      }
      return;
    }
    setDeleteAlbumTarget(album);
  }

  async function confirmVaultPick(vaultId: string, password?: string) {
    if (!vaultPick) return;
    setBusy(true);
    try {
      if (password) {
        await vault.unlock(vaultId, password);
      }

      if (vaultPick.kind === "assets") {
        const result = await api.lockAssetsToVault(vaultPick.ids, vaultId);
        clearSelection();
        setLightboxId(null);
        await loadAssets();
        await refreshStats();
        await vault.refreshStatus();
        await vault.refreshLocked();
        void refreshHistory();
        const warn = result.errors.length
          ? ` · ${result.errors.slice(0, 2).join("; ")}`
          : "";
        setError(`Moved ${result.locked} item(s) to the Locked folder${warn}`);
      } else {
        const result = await api.lockAlbumToVault(vaultPick.albumId, vaultId);
        await Promise.all([
          refreshAlbums(),
          loadAssets(),
          refreshStats(),
          vault.refreshStatus(),
          vault.refreshLocked(),
        ]);
        void refreshHistory();
        const warn = result.errors.length
          ? ` · ${result.errors.slice(0, 2).join("; ")}`
          : "";
        setError(
          `Moved “${vaultPick.albumName}” (${result.locked} item(s)) to the Locked folder${warn}`,
        );
      }
      setVaultPick(null);
    } catch (e) {
      setError(String(e));
      throw e;
    } finally {
      setBusy(false);
    }
  }

  const {
    toggleFavorite,
    favoriteSelected,
    rateSelected,
    labelSelected,
    rateAsset,
    labelAsset,
    deleteSelected,
    cleanupDupeGroup,
    cleanupExactDupes,
    trashBlurryAssets,
    restoreSelected,
    permanentlyDeleteSelected,
    removeMissingFromLibrary,
    emptyTrash,
    openExportInFolder,
    openLocalPath,
    exportSelectedZip,
    undo,
    redo,
  } = useAssetActions({
    view,
    assets,
    setAssets,
    stats,
    selected,
    selectedIds,
    setSelected,
    setLightboxId,
    setTimelineAssets,
    setDupeAssets,
    dupes,
    loadDuplicates,
    refreshStats,
    refreshHistory,
    refreshExports,
    loadAssets,
    setError,
    setBusy,
    confirmBeforeDeleting: prefs?.general.confirmBeforeDeleting ?? true,
  });

  const {
    gridRef,
    marquee,
    onGridPointerDown,
    onGridPointerMove,
    onGridPointerUp,
    cancelMarquee,
  } = useMarqueeSelection({
    selected,
    setSelected,
    pickingForAlbum,
    onOpenAsset: setLightboxId,
    onToggleAsset: toggleSelection,
    doubleClickOpensViewer: prefs?.general.doubleClickOpensViewer ?? false,
  });

  const {
    albumModal,
    setAlbumModal,
    albumName,
    setAlbumName,
    draggingIds,
    dropAlbumId,
    setDropAlbumId,
    onAssetDragStart,
    onAssetDragEnd,
    dropOnAlbum,
    dropOnNewAlbum,
    startPickingForAlbum,
    cancelAlbumPick,
    confirmAlbumPick,
    openCreateAlbumModal,
    openMoveAlbumModal,
    submitAlbumModal,
    moveToExistingAlbum,
  } = useAlbumWorkflows({
    albums,
    refreshAlbums,
    refreshHistory,
    loadAssets,
    selected,
    selectedIds,
    setSelected,
    setActiveAlbum,
    setView,
    setError,
    pickingForAlbum,
    setPickingForAlbum,
    cancelMarquee,
  });

  const { tagModal, setTagModal, tagName, setTagName, submitTagModal, applyExistingTag } =
    useTagWorkflows({ selectedIds, refreshTags, loadAssets, setError });

  const {
    importModal,
    setImportModal,
    onImportFiles,
    onImportFolder,
    cancelImport,
  } = useImportFlow({
    setBusy,
    setImportProgress,
    loadAssets,
    setError,
    prefs,
    setView,
  });

  // Persist / restore last view when the preference is on.
  useEffect(() => {
    if (!prefs || prefsLoading) return;
    if (!prefs.general.restorePreviousSession) return;
    try {
      const raw = localStorage.getItem("lumora.session.view");
      if (!raw) return;
      const allowed: View[] = [
        "home",
        "library",
        "recent",
        "recentViewed",
        "timeline",
        "albums",
        "tags",
        "savedSearches",
        "duplicates",
        "videos",
        "rawPhotos",
        "screenshots",
        "selfies",
        "panoramas",
        "documents",
        "receipts",
        "people",
        "places",
        "memories",
        "trash",
        "favorites",
        "watched",
        "activity",
        "exports",
        "settings",
      ];
      if (allowed.includes(raw as View)) {
        setView(raw as View);
      }
    } catch {
      /* ignore */
    }
    // Only on first prefs load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefsLoading]);

  useEffect(() => {
    if (!prefs?.general.restorePreviousSession) return;
    try {
      localStorage.setItem("lumora.session.view", view);
    } catch {
      /* ignore */
    }
  }, [view, prefs?.general.restorePreviousSession]);

  useKeyboardShortcuts({
    albumModal,
    tagModal,
    importModal,
    infoAssetId,
    setInfoAssetId,
    lightboxId,
    lightboxAsset,
    setLightboxId,
    showNextMedia,
    showPrevMedia,
    toggleFavorite,
    rateAsset,
    selectedIds,
    pickingForAlbum,
    cancelAlbumPick,
    clearSelection,
    setAlbumModal,
    setTagModal,
    setImportModal,
    view,
    restoreSelected,
    deleteSelected,
    assets,
    favoriteSelected,
    rateSelected,
    selectAllVisible,
    undo,
    redo,
  });

  function handleNavigate(id: View) {
    setView(id);
    if (id === "albums") {
      setActiveAlbum(null);
    }
    if (id === "people") {
      setActivePerson(null);
    }
    if (id === "places") {
      setActivePlace(null);
    }
    if (id === "memories") {
      setActiveMemory(null);
    }
    if (id !== "library" || !pickingForAlbum) {
      setSelected(new Set());
    }
    if (id !== "library" && id !== "albums") {
      setPickingForAlbum(null);
    }
  }

  function openMemory(memoryId: string) {
    setView("memories");
    setAssets([]);
    void openMemoryDetail(memoryId);
    setSelected(new Set());
  }

  return (
    <div className="app-shell">
      <Sidebar
        view={view}
        stats={stats}
        albumCount={albums.length}
        tagCount={tags.length}
        peopleCount={people.length}
        placeCount={places.length}
        memoryCount={memories.length}
        savedSearchCount={recentSearches.length}
        exportCount={exports.length}
        lockedCount={vault.status?.totalLockedCount ?? 0}
        smartCounts={smartCounts}
        busy={busy}
        onImport={() => setImportModal(true)}
        onNavigate={handleNavigate}
      />

      <div className="main">
        <Toolbar
          query={query}
          onQueryChange={setQuery}
          onSubmitSearch={() => void submitSearch()}
          recentSearches={recentSearches}
          onPickRecent={runRecentSearch}
          canUndo={!!history?.canUndo}
          canRedo={!!history?.canRedo}
          onUndo={() => void undo()}
          onRedo={() => void redo()}
          isTrashView={view === "trash"}
          emptyTrashDisabled={!stats?.inTrash && assets.length === 0}
          onEmptyTrash={() => void emptyTrash()}
        />

        {importProgress && (
          <ImportProgressBar
            progress={importProgress}
            pct={importPct}
            onCancel={() => void cancelImport()}
          />
        )}

        {pickingForAlbum && (
          <AlbumPickBar
            target={pickingForAlbum}
            selectedCount={selectedIds.length}
            onCancel={cancelAlbumPick}
            onConfirm={() => void confirmAlbumPick()}
          />
        )}

        {hasSelection && !pickingForAlbum && (
          <SelectionBar
            selectedCount={selectedIds.length}
            isTrashView={view === "trash"}
            busy={busy}
            onClearSelection={clearSelection}
            onRestore={() => void restoreSelected()}
            onPermanentDelete={(deleteFiles) =>
              void permanentlyDeleteSelected(deleteFiles)
            }
            onFavorite={(favorite) => void favoriteSelected(favorite)}
            onOpenTagModal={() => {
              setTagName("");
              setTagModal(true);
            }}
            onExportZip={() => void exportSelectedZip()}
            onOpenMoveAlbum={openMoveAlbumModal}
            onMoveToLocked={() => void moveSelectionToLocked()}
            onDelete={() => void deleteSelected()}
            onSelectAllVisible={selectAllVisible}
            onRate={(rating) => void rateSelected(rating)}
            onLabel={(color) => void labelSelected(color)}
          />
        )}

        <div className="content">
          {error && <p className="muted">{error}</p>}

          {view === "trash" && (
            <div className="trash-banner" role="note">
              <Icon name="trash" className="trash-banner-icon" />
              <span>
                Items in trash are removed from the library after{" "}
                <strong>{stats?.trashRetentionDays ?? 30} days</strong>.
                Original files stay on disk unless you delete them explicitly
                or empty the trash.
              </span>
            </div>
          )}

          {view === "albums" && activeAlbum && (
            <AlbumDetailHeader
              album={albums.find((a) => a.id === activeAlbum) ?? null}
              hasSelection={hasSelection}
              hasVaults={(vault.vaults.length ?? 0) > 0}
              lockBusy={busy}
              onBack={() => {
                setActiveAlbum(null);
                setSelected(new Set());
              }}
              onStartPicking={startPickingForAlbum}
              onOpenMoveAlbum={openMoveAlbumModal}
              onCreateLockedVault={createLockedVault}
              onAddToExistingVault={(album) =>
                void moveAlbumToLocked(album.id, album.name)
              }
              onDeleteAlbum={requestDeleteAlbum}
            />
          )}

          {view === "tags" && (
            <TagFilterBoard
              tags={tags}
              tagBrowse={tagBrowse}
              tagBrowseActive={tagBrowseActive}
              ratingCounts={ratingCounts}
              colorCounts={colorCounts}
              tagBrowseSummary={tagBrowseSummary}
              onToggleTag={toggleTagFilter}
              onToggleRating={toggleRatingFilter}
              onToggleColor={toggleColorFilter}
              onClearAll={clearTagBrowse}
            />
          )}

          {view === "timeline" && (
            <TimelineView
              timelineYears={timelineYears}
              timelineScaleYears={timelineScaleYears}
              timeline={timeline}
              timelineAssets={timelineAssets}
              timelineKey={timelineKey}
              timelineLoading={timelineLoading}
              timelineVisibleCount={timelineVisibleCount}
              timelineTotal={timeline.length}
              sentinelRef={timelineSentinelRef}
              selected={selected}
              onJumpToYear={jumpToYear}
              onSelectMonth={(month) => void selectTimelineGroup(month)}
              onImport={() => setImportModal(true)}
              onAssetDragStart={onAssetDragStart}
              onAssetDragEnd={onAssetDragEnd}
              onToggleSelect={toggleSelection}
              onOpen={setLightboxId}
              onToggleFavorite={(asset) => void toggleFavorite(asset)}
              onShowInfo={setInfoAssetId}
            />
          )}

          {view === "timeline" ? null : view === "home" ? (
            <HomeView
              stats={stats}
              smartCounts={smartCounts}
              recent={homeRecent}
              memories={memories}
              onRecentLoaded={setHomeRecent}
              onNavigate={handleNavigate}
              onImport={() => setImportModal(true)}
              onOpenAsset={setLightboxId}
              onOpenMemory={openMemory}
            />
          ) : view === "locked" ? (
            <LockedFolderView
              status={vault.status}
              vaults={vault.vaults}
              lockedAlbums={vault.lockedAlbums}
              lockedAssets={vault.lockedAssets}
              thumbs={vault.thumbs}
              openAlbumId={vault.openAlbumId}
              onOpenAlbum={vault.openAlbum}
              pendingUnlockId={vault.pendingUnlockId}
              creating={vault.creating}
              browsingContents={vault.browsingContents}
              onOpenVault={vault.openVault}
              onStartCreate={vault.startCreate}
              onCancelCreate={vault.cancelCreate}
              onCancelUnlock={vault.cancelUnlock}
              onBackToVaultList={vault.backToVaultList}
              onFinishSetup={vault.finishSetup}
              onSetup={vault.setup}
              onUnlock={vault.unlock}
              onRecover={vault.recover}
              onEnableRecovery={vault.enableRecovery}
              onLock={vault.lock}
              refreshLocked={vault.refreshLocked}
              refreshStatus={vault.refreshStatus}
              setError={setError}
            />
          ) : view === "watched" ? (
            <WatchedFoldersView
              folders={watchedFolders}
              loading={watchedLoading}
              busy={busy}
              onRefresh={() => void refreshWatched()}
              onAdd={() => void addWatchedFolder()}
              onRemove={(path) => void removeWatchedFolder(path)}
            />
          ) : view === "settings" ? (
            <SettingsView
              prefs={prefs}
              loading={prefsLoading}
              update={updatePrefs}
              watchedFolders={watchedFolders}
              watchedLoading={watchedLoading}
              busy={busy}
              onAddFolder={() => void addWatchedFolder()}
              onRemoveFolder={(path) => void removeWatchedFolder(path)}
              onRefreshWatched={() => void refreshWatched()}
              onOpenPath={(path, reveal) => void openLocalPath(path, reveal)}
              onOpenLocked={() => setView("locked")}
              vaultStatus={vault.status}
              appVersion={developerInfo?.appVersion ?? "—"}
              updater={updater}
            />
          ) : view === "savedSearches" ? (
            <RecentSearchesView
              searches={recentSearches}
              onRun={runRecentSearch}
              onDelete={(search) => {
                void removeRecentSearch(search.id).catch((e) =>
                  setError(String(e)),
                );
              }}
              onClear={() => {
                if (
                  !window.confirm(
                    "Clear all recent searches?\n\nYour photo library is not affected.",
                  )
                ) {
                  return;
                }
                void clearRecentSearches().catch((e) => setError(String(e)));
              }}
            />
          ) : view === "developer" ? (
            <DeveloperView
              info={developerInfo}
              loading={developerLoading}
              onRefresh={() => void refreshDeveloperInfo()}
              onOpenPath={(path, reveal) => void openLocalPath(path, reveal)}
              onViewActivity={() => setView("activity")}
            />
          ) : view === "activity" ? (
            <ActivityView
              history={history}
              onUndo={() => void undo()}
              onRedo={() => void redo()}
              onRefresh={() => void refreshHistory()}
            />
          ) : view === "exports" ? (
            <ExportsView
              exports={exports}
              onRefresh={() => void refreshExports()}
              onOpenInFolder={(path) => void openExportInFolder(path)}
              onBrowseLibrary={() => setView("library")}
            />
          ) : view === "albums" && !activeAlbum ? (
            <AlbumsGridView
              albums={albums}
              onCreateAlbum={openCreateAlbumModal}
              onOpenAlbum={(albumId) => {
                setActiveAlbum(albumId);
                setSelected(new Set());
              }}
            />
          ) : view === "people" && !activePerson ? (
            <PeopleView
              people={people}
              ignoredPeople={ignoredPeople}
              onRefresh={() => void refreshPeople()}
              onOpenPerson={(personId) => {
                setActivePerson(personId);
                setSelected(new Set());
              }}
              onNamePerson={(person) => {
                setPersonModal(person);
                setPersonName(person.name ?? "");
              }}
              onSetIgnored={(personId, ignored) => {
                void setPersonIgnored(personId, ignored);
              }}
            />
          ) : view === "places" && !activePlace ? (
            <PlacesView
              places={places}
              onRefresh={() => void refreshPlaces()}
              onOpenPlace={(label) => {
                setActivePlace(label);
                setSelected(new Set());
              }}
            />
          ) : view === "memories" && !activeMemory ? (
            <MemoriesView
              memories={memories}
              onRefresh={() => void refreshMemories()}
              onOpenMemory={(memoryId) => {
                setAssets([]);
                void openMemoryDetail(memoryId);
                setSelected(new Set());
              }}
            />
          ) : view === "duplicates" ? (
            <DuplicatesView
              dupes={dupes}
              dupeAssets={dupeAssets}
              blurry={blurry}
              onRefresh={() => void loadDuplicates()}
              onCleanupExact={() => void cleanupExactDupes()}
              onCleanupGroup={(group, keepId) =>
                void cleanupDupeGroup(group, keepId)
              }
              onTrashBlurry={(ids) => void trashBlurryAssets(ids)}
              onPreview={setLightboxId}
              onShowInfo={setInfoAssetId}
              onBrowseLibrary={() => setView("library")}
            />
          ) : (
            <>
              {LIBRARY_PAGE_META[view] &&
                !(view === "albums" && activeAlbum) &&
                !(view === "people" && activePerson) &&
                !(view === "places" && activePlace) &&
                !(view === "memories" && activeMemory) && (
                <PageHeader
                  title={LIBRARY_PAGE_META[view]!.title}
                  description={LIBRARY_PAGE_META[view]!.description}
                />
              )}
              {view === "places" && activePlace && (
                <PageHeader
                  title={activePlace}
                  description="Photos taken at this location."
                  actions={
                    <button
                      type="button"
                      onClick={() => {
                        setActivePlace(null);
                        setSelected(new Set());
                      }}
                    >
                      Back to places
                    </button>
                  }
                />
              )}
              {view === "memories" && activeMemory && (
                <PageHeader
                  title={
                    memories.find((m) => m.id === activeMemory)?.title ?? "Memory"
                  }
                  description={
                    memories.find((row) => row.id === activeMemory)?.insight ??
                    "Photos in this memory."
                  }
                  actions={
                    <>
                      <button
                        type="button"
                        className="primary"
                        disabled={savingMemoryAlbum || assets.length === 0}
                        onClick={() => {
                          void (async () => {
                            const album = await saveAsAlbum(activeMemory);
                            if (!album) return;
                            await refreshAlbums();
                            await refreshHistory();
                            setActiveAlbum(album.id);
                            setView("albums");
                            setActiveMemory(null);
                            setSelected(new Set());
                          })();
                        }}
                      >
                        {savingMemoryAlbum ? "Saving…" : "Save as album"}
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setActiveMemory(null);
                          setSelected(new Set());
                        }}
                      >
                        Back to memories
                      </button>
                    </>
                  }
                />
              )}
              {view === "people" && activePerson && (
                <PageHeader
                  title={
                    people.find((p) => p.id === activePerson)?.name?.trim() ||
                    "Unnamed person"
                  }
                  description="Photos this person appears in."
                  actions={
                    <>
                      <button
                        type="button"
                        onClick={() => {
                          setSelected(new Set());
                          void setPersonIgnored(activePerson, true);
                        }}
                      >
                        Ignore this person
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setActivePerson(null);
                          setSelected(new Set());
                        }}
                      >
                        Back to people
                      </button>
                    </>
                  }
                />
              )}
              {assets.length === 0 ? (
            <AssetEmptyState
              view={view}
              albums={albums}
              activeAlbum={activeAlbum}
              tagBrowseActive={tagBrowseActive}
              onClearTagBrowse={clearTagBrowse}
              query={query}
              onClearSearch={() => setQuery("")}
              isPicking={!!pickingForAlbum}
              onImport={() => setImportModal(true)}
              onBrowseLibrary={() => setView("library")}
              onStartPicking={startPickingForAlbum}
            />
          ) : (
            <LibraryGrid
              assets={assets}
              gridRef={gridRef}
              onNearEnd={hasMore ? loadMoreAssets : undefined}
              onPointerDown={onGridPointerDown}
              onPointerMove={onGridPointerMove}
              onPointerUp={onGridPointerUp}
              marquee={marquee}
              selected={selected}
              isTrashView={view === "trash"}
              trashRetentionDays={stats?.trashRetentionDays ?? 30}
              onAssetDragStart={onAssetDragStart}
              onAssetDragEnd={onAssetDragEnd}
              onToggleSelect={toggleSelection}
              onToggleFavorite={(asset) => void toggleFavorite(asset)}
              onShowInfo={setInfoAssetId}
            />
          )}
            </>
          )}
        </div>

        <StatusBar
          selectedCount={selectedIds.length}
          isTimelineView={view === "timeline"}
          visibleTimelineAssetCount={visibleTimelineAssetCount}
          assetCount={assets.length}
          progress={progress}
          isPicking={!!pickingForAlbum}
        />
      </div>

      {draggingIds.length > 0 && (
        <DropDock
          draggingCount={draggingIds.length}
          albums={albums}
          dropAlbumId={dropAlbumId}
          setDropAlbumId={setDropAlbumId}
          onDropAlbum={(album) => void dropOnAlbum(album)}
          onDropNew={dropOnNewAlbum}
        />
      )}

      {lightboxAsset && (
        <MediaViewer
          asset={lightboxAsset}
          index={viewerIndex}
          total={viewerList.length}
          onClose={() => setLightboxId(null)}
          onPrev={showPrevMedia}
          onNext={showNextMedia}
          onRate={rateAsset}
          onLabel={labelAsset}
          onToggleFavorite={toggleFavorite}
          onShowInfo={() => setInfoAssetId(lightboxAsset.id)}
          onRemoveFromLibrary={(asset) => void removeMissingFromLibrary(asset)}
          onEdited={(result) => {
            setAssets((rows) => {
              if (result.mode === "replace") {
                return rows.map((a) =>
                  a.id === result.asset.id ? result.asset : a,
                );
              }
              return [result.asset, ...rows.filter((a) => a.id !== result.asset.id)];
            });
            setLightboxId(result.asset.id);
            void refreshStats();
            void refreshHistory();
            setError(
              result.mode === "copy"
                ? `Saved copy${result.embeddingQueued ? " · re-embedding queued" : ""}`
                : `Saved edits${result.embeddingQueued ? " · re-embedding queued" : ""}`,
            );
          }}
        />
      )}

      {infoAsset && (
        <MediaInfoPanel
          asset={infoAsset}
          onClose={() => setInfoAssetId(null)}
        />
      )}

      {albumModal && (
        <AlbumModal
          mode={albumModal}
          selectedCount={selectedIds.length}
          albums={albums}
          name={albumName}
          onNameChange={setAlbumName}
          onClose={() => setAlbumModal(null)}
          onSubmit={() => void submitAlbumModal()}
          onPickExisting={(albumId) => void moveToExistingAlbum(albumId)}
        />
      )}

      {deleteAlbumTarget && (
        <DeleteAlbumDialog
          album={deleteAlbumTarget}
          busy={busy}
          onCancel={() => setDeleteAlbumTarget(null)}
          onConfirm={(deleteAssets) =>
            void performDeleteAlbum(deleteAlbumTarget, deleteAssets)
          }
        />
      )}

      {personModal && (
        <PersonModal
          person={personModal}
          people={people}
          name={personName}
          onNameChange={setPersonName}
          onClose={() => setPersonModal(null)}
          onSubmit={() => {
            void (async () => {
              try {
                await api.renamePerson(personModal.id, personName);
                setPersonModal(null);
                await refreshPeople();
              } catch (e) {
                setError(String(e));
              }
            })();
          }}
          onMergeInto={(intoId) => {
            void (async () => {
              try {
                await api.mergePeople(intoId, personModal.id);
                setPersonModal(null);
                if (activePerson === personModal.id) {
                  setActivePerson(intoId);
                }
                await refreshPeople();
                void loadAssets();
              } catch (e) {
                setError(String(e));
              }
            })();
          }}
        />
      )}

      {tagModal && (
        <TagModal
          selectedCount={selectedIds.length}
          tags={tags}
          name={tagName}
          onNameChange={setTagName}
          onClose={() => setTagModal(false)}
          onSubmit={() => void submitTagModal()}
          onApplyExisting={(tag) => void applyExistingTag(tag)}
        />
      )}

      {importModal && (
        <ImportModal
          onClose={() => setImportModal(false)}
          onChooseFiles={() => void onImportFiles()}
          onChooseFolder={() => void onImportFolder()}
        />
      )}

      {vaultPick && (
        <VaultPickerDialog
          vaults={vault.vaults}
          title={
            vaultPick.kind === "album"
              ? `Add “${vaultPick.albumName}” to vault`
              : "Move to Locked folder"
          }
          busy={busy}
          onCancel={() => setVaultPick(null)}
          onConfirm={confirmVaultPick}
        />
      )}
    </div>
  );
}
