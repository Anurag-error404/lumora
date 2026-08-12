import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type HistorySnapshot } from "../lib/tauri";
import { idleDefer } from "../lib/idleDefer";
import type { View } from "../types/app";

/** Undo/redo stacks and the activity feed snapshot. */
export function useHistoryFeed({
  view,
  setError,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [history, setHistory] = useState<HistorySnapshot | null>(null);

  const refreshHistory = useCallback(async () => {
    try {
      const snap = await api.getHistory();
      setHistory(snap);
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    if (view !== "activity") return;
    return idleDefer(() => void refreshHistory());
  }, [view, refreshHistory]);

  return { history, refreshHistory };
}
