import { useEffect, useRef, useState } from "react";
import { api, type IndexProgress } from "../lib/tauri";

const IDLE_POLL_MS = 5000;
const ACTIVE_POLL_MS = 1000;

/** Polls the background indexer — slow when idle, 1 Hz while work is pending. */
export function useIndexProgress(): IndexProgress | null {
  const [progress, setProgress] = useState<IndexProgress | null>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    let cancelled = false;

    const schedule = (ms: number) => {
      if (timerRef.current != null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(tick, ms);
    };

    const tick = () => {
      void api
        .getIndexProgress()
        .then((next) => {
          if (cancelled) return;
          setProgress(next);
          const busy = next.pending > 0 || next.running;
          schedule(busy ? ACTIVE_POLL_MS : IDLE_POLL_MS);
        })
        .catch(() => {
          if (!cancelled) schedule(IDLE_POLL_MS);
        });
    };

    tick();
    return () => {
      cancelled = true;
      if (timerRef.current != null) window.clearTimeout(timerRef.current);
    };
  }, []);

  return progress;
}
