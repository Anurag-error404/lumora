import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { PluginRunProgressEvent } from "../lib/tauri";

/** Listens for backend plugin-run-progress events. */
export function usePluginRunProgress() {
  const [pluginRun, setPluginRun] = useState<PluginRunProgressEvent | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<PluginRunProgressEvent>("plugin-run-progress", (event) => {
      setPluginRun(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const pluginRunPct =
    pluginRun && pluginRun.total > 0
      ? Math.min(100, Math.round((pluginRun.current / pluginRun.total) * 100))
      : pluginRun?.phase === "starting"
        ? 0
        : null;

  return { pluginRun, setPluginRun, pluginRunPct };
}
