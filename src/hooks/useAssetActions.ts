import type { Dispatch, SetStateAction } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  api,
  type AssetSummary,
  type DuplicateGroup,
  type LibraryStats,
} from "../lib/tauri";
import type { View } from "../types/app";

/**
 * Mutating actions on assets: favourites, ratings, colour labels, trash,
 * duplicate cleanup, ZIP export, and undo/redo. Optimistically patches every
 * cached list (library page, timeline groups, duplicate map).
 */
export function useAssetActions({
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
  confirmBeforeDeleting = true,
}: {
  view: View;
  assets: AssetSummary[];
  setAssets: Dispatch<SetStateAction<AssetSummary[]>>;
  stats: LibraryStats | null;
  selected: Set<string>;
  selectedIds: string[];
  setSelected: Dispatch<SetStateAction<Set<string>>>;
  setLightboxId: Dispatch<SetStateAction<string | null>>;
  setTimelineAssets: Dispatch<SetStateAction<Record<string, AssetSummary[]>>>;
  setDupeAssets: Dispatch<SetStateAction<Map<string, AssetSummary>>>;
  dupes: DuplicateGroup[];
  loadDuplicates: () => Promise<void>;
  refreshStats: () => Promise<void>;
  refreshHistory: () => Promise<void>;
  refreshExports: () => Promise<void>;
  loadAssets: () => Promise<void>;
  setError: Dispatch<SetStateAction<string | null>>;
  setBusy: Dispatch<SetStateAction<boolean>>;
  confirmBeforeDeleting?: boolean;
}) {
  async function toggleFavorite(asset: AssetSummary) {
    const next = !asset.favorite;
    setAssets((rows) =>
      rows.map((a) => (a.id === asset.id ? { ...a, favorite: next } : a)),
    );
    setTimelineAssets((groups) =>
      Object.fromEntries(
        Object.entries(groups).map(([key, rows]) => [
          key,
          rows.map((a) =>
            a.id === asset.id ? { ...a, favorite: next } : a,
          ),
        ]),
      ),
    );
    try {
      await api.setFavorite(asset.id, next);
      await refreshStats();
      void refreshHistory();
      if (view === "favorites" && !next) {
        await loadAssets();
      }
    } catch (e) {
      setError(String(e));
      setAssets((rows) =>
        rows.map((a) => (a.id === asset.id ? { ...a, favorite: asset.favorite } : a)),
      );
    }
  }

  async function favoriteSelected(favorite: boolean) {
    if (!selectedIds.length) return;
    setAssets((rows) =>
      rows.map((a) => (selected.has(a.id) ? { ...a, favorite } : a)),
    );
    setTimelineAssets((groups) =>
      Object.fromEntries(
        Object.entries(groups).map(([key, rows]) => [
          key,
          rows.map((a) => (selected.has(a.id) ? { ...a, favorite } : a)),
        ]),
      ),
    );
    try {
      await api.setFavorites(selectedIds, favorite);
      await refreshStats();
      void refreshHistory();
      if (view === "favorites") await loadAssets();
    } catch (e) {
      setError(String(e));
      setTimelineAssets({});
      await loadAssets();
    }
  }

  /** Patch matching assets in every cached list without a full reload. */
  function patchAssets(ids: Set<string>, patch: Partial<AssetSummary>) {
    setAssets((rows) =>
      rows.map((a) => (ids.has(a.id) ? { ...a, ...patch } : a)),
    );
    setTimelineAssets((groups) =>
      Object.fromEntries(
        Object.entries(groups).map(([key, rows]) => [
          key,
          rows.map((a) => (ids.has(a.id) ? { ...a, ...patch } : a)),
        ]),
      ),
    );
    setDupeAssets((map) => {
      if (![...ids].some((id) => map.has(id))) return map;
      const next = new Map(map);
      for (const id of ids) {
        const asset = next.get(id);
        if (asset) next.set(id, { ...asset, ...patch });
      }
      return next;
    });
  }

  async function rateSelected(rating: number) {
    if (!selectedIds.length) return;
    try {
      await api.setRatings(selectedIds, rating);
      patchAssets(selected, { rating });
      void refreshHistory();
      setError(
        rating === 0
          ? `Cleared rating on ${selectedIds.length} photo(s)`
          : `Rated ${selectedIds.length} photo(s) ${rating}★`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function labelSelected(colorLabel: string | null) {
    if (!selectedIds.length) return;
    try {
      await api.setColorLabels(selectedIds, colorLabel);
      patchAssets(selected, { colorLabel });
      void refreshHistory();
      setError(
        colorLabel
          ? `Labelled ${selectedIds.length} photo(s) ${colorLabel}`
          : `Removed colour label from ${selectedIds.length} photo(s)`,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  async function rateAsset(asset: AssetSummary, rating: number) {
    try {
      await api.setRatings([asset.id], rating);
      patchAssets(new Set([asset.id]), { rating });
      void refreshHistory();
    } catch (e) {
      setError(String(e));
    }
  }

  async function labelAsset(asset: AssetSummary, colorLabel: string | null) {
    try {
      await api.setColorLabels([asset.id], colorLabel);
      patchAssets(new Set([asset.id]), { colorLabel });
      void refreshHistory();
    } catch (e) {
      setError(String(e));
    }
  }

  async function deleteSelected() {
    if (!selectedIds.length) return;
    if (
      confirmBeforeDeleting &&
      !window.confirm(
        `Move ${selectedIds.length} item(s) to Trash?\n\nYou can restore them from Trash or undo with ⌘Z.`,
      )
    ) {
      return;
    }
    await api.softDeleteAssets(selectedIds);
    setSelected(new Set());
    setLightboxId(null);
    await refreshHistory();
    await loadAssets();
  }

  async function cleanupDupeGroup(group: DuplicateGroup, keepId: string) {
    const toTrash = group.assetIds.filter((id) => id !== keepId);
    if (!toTrash.length) return;
    try {
      await api.softDeleteAssets(toTrash);
      setError(`Kept 1, moved ${toTrash.length} duplicate(s) to trash (undo with ⌘Z)`);
      await loadDuplicates();
      await refreshStats();
      await refreshHistory();
    } catch (e) {
      setError(String(e));
    }
  }

  async function cleanupAllDupes() {
    if (!dupes.length) return;
    // An asset can appear in both an exact and a near group; never trash a keeper.
    const keep = new Set(dupes.map((g) => g.assetIds[0]));
    const toTrash = [
      ...new Set(
        dupes.flatMap((g) => g.assetIds.filter((id) => !keep.has(id))),
      ),
    ];
    if (!toTrash.length) return;
    if (
      !window.confirm(
        `Clean up ${dupes.length} duplicate group(s)?\n\nThis keeps the first photo of each group and moves ${toTrash.length} duplicate(s) to trash. You can restore them from Trash or undo with ⌘Z.`,
      )
    )
      return;
    try {
      await api.softDeleteAssets(toTrash);
      setError(`Cleaned up: moved ${toTrash.length} duplicate(s) to trash`);
      await loadDuplicates();
      await refreshStats();
    } catch (e) {
      setError(String(e));
    }
  }

  async function restoreSelected() {
    if (!selectedIds.length) return;
    await api.restoreAssets(selectedIds);
    setSelected(new Set());
    await loadAssets();
  }

  async function permanentlyDeleteSelected(deleteFiles: boolean) {
    if (!selectedIds.length) return;
    const count = selectedIds.length;
    const message = deleteFiles
      ? `Permanently delete ${count} item(s) from the library AND remove the original file(s) from disk?\n\nThis cannot be undone.`
      : `Remove ${count} item(s) from the library permanently?\n\nOriginal files on disk will be kept.`;
    if (!window.confirm(message)) return;
    try {
      const result = await api.permanentlyDeleteAssets(selectedIds, deleteFiles);
      setSelected(new Set());
      setLightboxId(null);
      const extra =
        result.errors.length > 0 ? ` · warnings: ${result.errors.join("; ")}` : "";
      setError(
        deleteFiles
          ? `Deleted ${result.filesDeleted} file(s) from disk; removed ${result.removedFromLibrary} from library${extra}`
          : `Removed ${result.removedFromLibrary} from library (files kept)${extra}`,
      );
      await refreshHistory();
      await loadAssets();
    } catch (e) {
      setError(String(e));
    }
  }

  async function emptyTrash() {
    const count = stats?.inTrash ?? assets.length;
    if (!count) {
      setError("Trash is already empty");
      return;
    }
    if (
      !window.confirm(
        `Empty trash?\n\nThis permanently deletes ${count} item(s) from the library AND removes the original file(s) from disk.\n\nThis cannot be undone.`,
      )
    )
      return;
    try {
      const result = await api.emptyTrash();
      setSelected(new Set());
      setLightboxId(null);
      const extra =
        result.errors.length > 0 ? ` · warnings: ${result.errors.join("; ")}` : "";
      setError(
        `Emptied trash: deleted ${result.filesDeleted} file(s), removed ${result.removedFromLibrary} from library${extra}`,
      );
      await refreshHistory();
      await loadAssets();
      await refreshStats();
    } catch (e) {
      setError(String(e));
    }
  }

  async function openExportInFolder(path: string) {
    try {
      await revealItemInDir(path);
    } catch {
      try {
        const parent = path.replace(/[/\\][^/\\]+$/, "");
        if (!parent || parent === path) {
          throw new Error("Could not determine the export folder");
        }
        await openPath(parent);
        setError("Export file missing — opened containing folder instead");
      } catch (e) {
        setError(String(e));
      }
    }
  }

  async function openLocalPath(path: string, reveal = false) {
    try {
      if (reveal) await revealItemInDir(path);
      else await openPath(path);
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportSelectedZip() {
    if (!selectedIds.length) return;
    const stamp = new Date().toISOString().slice(0, 10);
    const dest = await save({
      title: "Export selected media",
      defaultPath: `LUMORA-export-${stamp}.zip`,
      filters: [{ name: "ZIP archive", extensions: ["zip"] }],
    });
    if (!dest) return;
    setBusy(true);
    try {
      const result = await api.exportAssetsZip(selectedIds, dest);
      const warn =
        result.missing > 0 || result.errors.length > 0
          ? ` · ${result.missing} missing${
              result.errors.length
                ? ` · ${result.errors.slice(0, 2).join("; ")}`
                : ""
            }`
          : "";
      setError(
        `Exported ${result.exported} file(s) to ${result.path}${warn}`,
      );
      await refreshHistory();
      await refreshExports();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function undo() {
    const ok = await api.undoLast();
    if (!ok) {
      setError("Nothing to undo");
      return;
    }
    await refreshHistory();
    await loadAssets();
  }

  async function redo() {
    const ok = await api.redoLast();
    if (!ok) {
      setError("Nothing to redo");
      return;
    }
    await refreshHistory();
    await loadAssets();
  }

  return {
    toggleFavorite,
    favoriteSelected,
    rateSelected,
    labelSelected,
    rateAsset,
    labelAsset,
    deleteSelected,
    cleanupDupeGroup,
    cleanupAllDupes,
    restoreSelected,
    permanentlyDeleteSelected,
    emptyTrash,
    openExportInFolder,
    openLocalPath,
    exportSelectedZip,
    undo,
    redo,
  };
}
