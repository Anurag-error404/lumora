import {
  useState,
  type Dispatch,
  type DragEvent as ReactDragEvent,
  type SetStateAction,
} from "react";
import { api, type Album, type AssetSummary } from "../lib/tauri";
import type { AlbumModalMode, AlbumPickTarget, View } from "../types/app";

/**
 * Album workflows: the create/move modal, "pick photos for album" mode, and
 * native drag-drop of selected photos onto the album drop dock.
 */
export function useAlbumWorkflows({
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
}: {
  albums: Album[];
  refreshAlbums: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  loadAssets: () => Promise<void>;
  selected: Set<string>;
  selectedIds: string[];
  setSelected: Dispatch<SetStateAction<Set<string>>>;
  setActiveAlbum: Dispatch<SetStateAction<string | null>>;
  setView: Dispatch<SetStateAction<View>>;
  setError: Dispatch<SetStateAction<string | null>>;
  pickingForAlbum: AlbumPickTarget | null;
  setPickingForAlbum: Dispatch<SetStateAction<AlbumPickTarget | null>>;
  cancelMarquee: () => void;
}) {
  const [albumModal, setAlbumModal] = useState<AlbumModalMode>(null);
  const [albumName, setAlbumName] = useState("");
  const [draggingIds, setDraggingIds] = useState<string[]>([]);
  const [dropAlbumId, setDropAlbumId] = useState<string | null>(null);

  function clearSelection() {
    setSelected(new Set());
  }

  /** Native drag of selected photos onto an album in the drop dock. */
  function onAssetDragStart(
    e: ReactDragEvent<HTMLDivElement>,
    asset: AssetSummary,
  ) {
    const ids = selected.has(asset.id) ? selectedIds : [asset.id];
    // Cancel any in-flight marquee tracking; native DnD takes over the pointer.
    cancelMarquee();
    e.dataTransfer.setData("text/plain", `${ids.length} photo(s)`);
    e.dataTransfer.effectAllowed = "copy";
    setDraggingIds(ids);
  }

  function onAssetDragEnd() {
    setDraggingIds([]);
    setDropAlbumId(null);
  }

  async function dropOnAlbum(album: Album) {
    const ids = draggingIds;
    setDraggingIds([]);
    setDropAlbumId(null);
    if (!ids.length) return;
    try {
      const added = await api.addAssetsToAlbum(album.id, ids);
      await refreshAlbums();
      void refreshHistory();
      setError(
        added > 0
          ? `Added ${added} photo(s) to “${album.name}”`
          : `Those photos were already in “${album.name}”`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  function dropOnNewAlbum() {
    const ids = draggingIds;
    setDraggingIds([]);
    setDropAlbumId(null);
    if (!ids.length) return;
    setSelected(new Set(ids));
    setAlbumName("");
    setAlbumModal("move");
  }

  function startPickingForAlbum(album: { id: string; name: string }) {
    setPickingForAlbum({ id: album.id, name: album.name });
    setSelected(new Set());
    setView("library");
    setError(null);
  }

  function cancelAlbumPick() {
    setPickingForAlbum(null);
  }

  async function confirmAlbumPick() {
    if (!pickingForAlbum || !selectedIds.length) return;
    const target = pickingForAlbum;
    try {
      const count = await api.addAssetsToAlbum(target.id, selectedIds);
      setPickingForAlbum(null);
      clearSelection();
      await refreshAlbums();
      await refreshHistory();
      setActiveAlbum(target.id);
      setView("albums");
      setError(
        count > 0
          ? `Added ${count} photo(s) to “${target.name}”`
          : `Those photos were already in “${target.name}”`,
      );
      await loadAssets();
    } catch (e) {
      setError(String(e));
    }
  }

  function openCreateAlbumModal() {
    setAlbumName("");
    setAlbumModal("create");
  }

  function openMoveAlbumModal() {
    if (!selectedIds.length) return;
    setAlbumName("");
    void refreshAlbums();
    setAlbumModal("move");
  }

  async function submitAlbumModal() {
    const name = albumName.trim();
    try {
      if (albumModal === "create") {
        if (!name) {
          setError("Enter an album name");
          return;
        }
        const album =
          selectedIds.length > 0
            ? await api.createAlbumWithAssets(name, selectedIds)
            : await api.createAlbum(name);
        await refreshAlbums();
        await refreshHistory();
        setActiveAlbum(album.id);
        setAlbumModal(null);
        if (selectedIds.length) {
          setView("albums");
          setError(
            `Created album “${album.name}” with ${selectedIds.length} photo(s)`,
          );
          clearSelection();
          await loadAssets();
        } else {
          // Empty album → jump straight into library pick mode.
          startPickingForAlbum(album);
        }
        return;
      }

      if (albumModal === "move") {
        if (!selectedIds.length) return;
        if (name) {
          const album = await api.createAlbumWithAssets(name, selectedIds);
          await refreshAlbums();
          await refreshHistory();
          setActiveAlbum(album.id);
          setView("albums");
          setAlbumModal(null);
          setError(`Moved ${selectedIds.length} item(s) into “${album.name}”`);
          clearSelection();
          await loadAssets();
          return;
        }
        setError("Enter a new album name, or choose an existing album below");
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function moveToExistingAlbum(albumId: string) {
    if (!selectedIds.length) return;
    try {
      const added = await api.addAssetsToAlbum(albumId, selectedIds);
      await refreshAlbums();
      await refreshHistory();
      const album = albums.find((a) => a.id === albumId);
      setAlbumModal(null);
      setActiveAlbum(albumId);
      setView("albums");
      setError(
        `Added ${added || selectedIds.length} item(s) to “${album?.name ?? "album"}”`,
      );
      clearSelection();
      await loadAssets();
    } catch (e) {
      setError(String(e));
    }
  }

  return {
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
  };
}
