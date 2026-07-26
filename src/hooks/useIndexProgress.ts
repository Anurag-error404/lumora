import { useEffect, useState } from "react";
import { api, type IndexProgress } from "../lib/tauri";

/** Polls the background indexer once a second. */
export function useIndexProgress(): IndexProgress | null {
  const [progress, setProgress] = useState<IndexProgress | null>(null);

  useEffect(() => {
    const id = window.setInterval(() => {
      void api.getIndexProgress().then(setProgress).catch(() => undefined);
    }, 1000);
    return () => window.clearInterval(id);
  }, []);

  return progress;
}
