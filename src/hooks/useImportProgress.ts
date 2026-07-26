import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { ImportProgressEvent } from "../lib/tauri";

/** Listens for backend import-progress events and derives a percentage. */
export function useImportProgress() {
  const [importProgress, setImportProgress] =
    useState<ImportProgressEvent | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<ImportProgressEvent>("import-progress", (event) => {
      setImportProgress(event.payload);
      if (event.payload.phase === "done" || event.payload.phase === "cancelled") {
        window.setTimeout(() => setImportProgress(null), 1200);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const importPct =
    importProgress && importProgress.total > 0
      ? Math.round((importProgress.current / importProgress.total) * 100)
      : importProgress?.phase === "scanning"
        ? 0
        : null;

  return { importProgress, setImportProgress, importPct };
}
