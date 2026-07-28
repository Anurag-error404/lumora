import { useCallback, useEffect, useRef, useState } from "react";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import type { Preferences } from "../lib/tauri";

export type UpdateStatus =
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "downloading"
  | "ready"
  | "error";

export type UpdateProgress = {
  downloaded: number;
  contentLength: number | null;
};

function isDevRuntime(): boolean {
  return import.meta.env.DEV;
}

/** In-app updater: check GitHub Releases, download, install, relaunch. */
export function useAppUpdater(prefs: Preferences | null) {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [available, setAvailable] = useState<Update | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const autoChecked = useRef(false);
  const pending = useRef<Update | null>(null);

  const clearPending = useCallback(() => {
    const update = pending.current;
    pending.current = null;
    if (update) {
      void update.close().catch(() => {});
    }
  }, []);

  const checkForUpdates = useCallback(async (opts?: { silent?: boolean }) => {
    if (isDevRuntime()) {
      if (!opts?.silent) {
        setStatus("upToDate");
        setError(null);
        setAvailable(null);
      }
      return null;
    }

    setStatus("checking");
    setError(null);
    setProgress(null);
    clearPending();
    setAvailable(null);

    try {
      const update = await check();
      if (!update) {
        setStatus("upToDate");
        return null;
      }
      pending.current = update;
      setAvailable(update);
      setStatus("available");
      return update;
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      if (opts?.silent) {
        setStatus("idle");
        setError(null);
      } else {
        setError(message);
        setStatus("error");
      }
      return null;
    }
  }, [clearPending]);

  const downloadAndInstall = useCallback(async () => {
    const update = pending.current ?? available;
    if (!update) return;

    setStatus("downloading");
    setError(null);
    setProgress({ downloaded: 0, contentLength: null });

    let downloaded = 0;
    let contentLength: number | null = null;

    try {
      await update.downloadAndInstall((event: DownloadEvent) => {
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? null;
            downloaded = 0;
            setProgress({ downloaded, contentLength });
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            setProgress({ downloaded, contentLength });
            break;
          case "Finished":
            setProgress({ downloaded, contentLength });
            break;
        }
      });
      pending.current = null;
      setAvailable(null);
      setStatus("ready");
      await relaunch();
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      setStatus("error");
    }
  }, [available]);

  // Startup / preference-driven auto-check.
  useEffect(() => {
    if (!prefs || autoChecked.current) return;
    if (!prefs.updates.checkAutomatically) return;
    autoChecked.current = true;

    void (async () => {
      const update = await checkForUpdates({ silent: true });
      if (update && prefs.updates.downloadInBackground) {
        await downloadAndInstall();
      }
    })();
  }, [prefs, checkForUpdates, downloadAndInstall]);

  useEffect(() => () => clearPending(), [clearPending]);

  return {
    status,
    available,
    error,
    progress,
    checkForUpdates,
    downloadAndInstall,
    isDev: isDevRuntime(),
  };
}
