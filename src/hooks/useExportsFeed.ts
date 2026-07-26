import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type ExportRecord } from "../lib/tauri";
import type { View } from "../types/app";

/** Recent ZIP export records. */
export function useExportsFeed({
  view,
  setError,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [exports, setExports] = useState<ExportRecord[]>([]);

  const refreshExports = useCallback(async () => {
    try {
      setExports(await api.listExports(50));
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    void refreshExports();
  }, [refreshExports]);

  useEffect(() => {
    if (view === "exports") void refreshExports();
  }, [view, refreshExports]);

  return { exports, refreshExports };
}
