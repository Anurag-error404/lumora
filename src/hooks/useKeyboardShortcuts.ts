import { useEffect, type Dispatch, type SetStateAction } from "react";
import type { AssetSummary } from "../lib/tauri";
import type { AlbumModalMode, AlbumPickTarget, View } from "../types/app";

/**
 * Global keyboard shortcuts. Intentionally registered without a dependency
 * array so the handler always closes over the latest render's state, matching
 * the original inline effect in App.
 */
export function useKeyboardShortcuts({
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
}: {
  albumModal: AlbumModalMode;
  tagModal: boolean;
  importModal: boolean;
  infoAssetId: string | null;
  setInfoAssetId: Dispatch<SetStateAction<string | null>>;
  lightboxId: string | null;
  lightboxAsset: AssetSummary | null;
  setLightboxId: Dispatch<SetStateAction<string | null>>;
  showNextMedia: () => void;
  showPrevMedia: () => void;
  toggleFavorite: (asset: AssetSummary) => Promise<void>;
  rateAsset: (asset: AssetSummary, rating: number) => Promise<void>;
  selectedIds: string[];
  pickingForAlbum: AlbumPickTarget | null;
  cancelAlbumPick: () => void;
  clearSelection: () => void;
  setAlbumModal: Dispatch<SetStateAction<AlbumModalMode>>;
  setTagModal: Dispatch<SetStateAction<boolean>>;
  setImportModal: Dispatch<SetStateAction<boolean>>;
  view: View;
  restoreSelected: () => Promise<void>;
  deleteSelected: () => Promise<void>;
  assets: AssetSummary[];
  favoriteSelected: (favorite: boolean) => Promise<void>;
  rateSelected: (rating: number) => Promise<void>;
  selectAllVisible: () => void;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
}) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
      if (albumModal || tagModal || importModal) return;
      if (infoAssetId) {
        if (e.key === "Escape") setInfoAssetId(null);
        return;
      }

      // Viewer owns the keyboard while it is open.
      if (lightboxId && lightboxAsset) {
        if (e.key === "Escape") {
          setLightboxId(null);
        } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
          e.preventDefault();
          showNextMedia();
        } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
          e.preventDefault();
          showPrevMedia();
        } else if (e.key === " ") {
          e.preventDefault();
          setLightboxId(null);
        } else if (e.key.toLowerCase() === "f") {
          e.preventDefault();
          void toggleFavorite(lightboxAsset);
        } else if (
          /^[0-5]$/.test(e.key) &&
          !e.metaKey &&
          !e.ctrlKey &&
          !e.altKey
        ) {
          e.preventDefault();
          void rateAsset(lightboxAsset, Number(e.key));
        }
        return;
      }

      if (e.key === " " && selectedIds[0]) {
        e.preventDefault();
        setLightboxId((cur) => (cur ? null : selectedIds[0]));
      } else if (e.key === "Escape") {
        setLightboxId(null);
        if (pickingForAlbum) {
          cancelAlbumPick();
        } else {
          clearSelection();
        }
        setAlbumModal(null);
        setTagModal(false);
        setImportModal(false);
      } else if (e.key === "Delete" || e.key === "Backspace") {
        if (view === "trash") void restoreSelected();
        else void deleteSelected();
      } else if (e.key.toLowerCase() === "f" && selectedIds.length) {
        e.preventDefault();
        const anyUnfav = selectedIds.some(
          (id) => !assets.find((a) => a.id === id)?.favorite,
        );
        void favoriteSelected(anyUnfav);
      } else if (
        /^[0-5]$/.test(e.key) &&
        selectedIds.length &&
        !e.metaKey &&
        !e.ctrlKey &&
        !e.altKey &&
        view !== "trash"
      ) {
        e.preventDefault();
        void rateSelected(Number(e.key));
      } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        selectAllVisible();
      } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) void redo();
        else void undo();
      } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "y") {
        e.preventDefault();
        void redo();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
}
