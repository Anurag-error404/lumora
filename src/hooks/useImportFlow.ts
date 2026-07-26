import { useState, type Dispatch, type SetStateAction } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type ImportProgressEvent } from "../lib/tauri";
import { MEDIA_DIALOG_FILTERS } from "../lib/constants";

/** The import modal plus the file/folder pickers and import execution. */
export function useImportFlow({
  setBusy,
  setImportProgress,
  loadAssets,
  setError,
}: {
  setBusy: Dispatch<SetStateAction<boolean>>;
  setImportProgress: Dispatch<SetStateAction<ImportProgressEvent | null>>;
  loadAssets: () => Promise<void>;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [importModal, setImportModal] = useState(false);

  async function runImport(paths: string[]) {
    if (!paths.length) return;
    setImportModal(false);
    setBusy(true);
    setImportProgress({
      current: 0,
      total: 0,
      path: paths.length === 1 ? paths[0] : `${paths.length} items`,
      phase: "scanning",
    });
    try {
      const result = await api.importPaths(paths);
      setError(
        `Imported: scanned ${result.scanned}, inserted ${result.inserted}, updated ${result.updated}`,
      );
      await loadAssets();
    } catch (e) {
      setError(String(e));
      setImportProgress(null);
    } finally {
      setBusy(false);
    }
  }

  async function onImportFiles() {
    const selected = await open({
      title: "Import photos or videos",
      multiple: true,
      directory: false,
      filters: MEDIA_DIALOG_FILTERS,
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    await runImport(paths);
  }

  async function onImportFolder() {
    const selected = await open({
      title: "Import folder",
      directory: true,
      multiple: false,
    });
    if (!selected || Array.isArray(selected)) return;
    await runImport([selected]);
  }

  return { importModal, setImportModal, onImportFiles, onImportFolder };
}
