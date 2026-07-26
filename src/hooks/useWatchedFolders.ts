import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/tauri";
import type { View } from "../types/app";

/** Watched-folder list for the manage UI. */
export function useWatchedFolders({
  view,
  setError,
  setBusy,
  onImported,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
  setBusy: Dispatch<SetStateAction<boolean>>;
  onImported: () => Promise<void>;
}) {
  const [folders, setFolders] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setFolders(await api.listWatchedFolders());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [setError]);

  useEffect(() => {
    if (view === "watched" || view === "settings") void refresh();
  }, [view, refresh]);

  async function addFolder() {
    const selected = await open({
      title: "Add watched folder",
      directory: true,
      multiple: false,
    });
    if (!selected || Array.isArray(selected)) return;
    setBusy(true);
    try {
      const result = await api.importPaths([selected]);
      setError(
        `Watching ${selected} — scanned ${result.scanned}, inserted ${result.inserted}, updated ${result.updated}`,
      );
      await refresh();
      await onImported();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeFolder(path: string) {
    setBusy(true);
    try {
      await api.removeWatchedFolder(path);
      setError(`Stopped watching ${path}`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return { folders, loading, refresh, addFolder, removeFolder };
}
