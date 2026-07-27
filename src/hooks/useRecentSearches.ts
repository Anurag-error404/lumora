import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type SavedSearch } from "../lib/tauri";
import { idleDefer } from "../lib/idleDefer";
import type { View } from "../types/app";

/** Recent search history — auto-recorded when a query is run. */
export function useRecentSearches({
  view,
  setError,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [recentSearches, setRecentSearches] = useState<SavedSearch[]>([]);

  const refresh = useCallback(async () => {
    try {
      setRecentSearches(await api.listSavedSearches());
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => idleDefer(() => void refresh()), [refresh]);

  useEffect(() => {
    if (view === "savedSearches" || view === "home") void refresh();
  }, [refresh, view]);

  async function record(query: string) {
    const q = query.trim();
    if (!q) return;
    await api.recordRecentSearch(q);
    await refresh();
  }

  async function remove(id: string) {
    await api.deleteSavedSearch(id);
    await refresh();
  }

  async function clear() {
    await api.clearRecentSearches();
    await refresh();
  }

  return { recentSearches, refresh, record, remove, clear };
}
