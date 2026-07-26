import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import {
  api,
  isSmartCollectionKind,
  type AssetSummary,
  type LibraryStats,
  type SmartCounts,
  type TagBrowseFilter,
} from "../lib/tauri";
import { PAGE_SIZE } from "../lib/constants";
import type { View } from "../types/app";

/** Tokens that mean the query is FTS/filter syntax, not natural language. */
function hasStructuredFilters(query: string): boolean {
  return /\b(camera:|lens:|rating[><=]|before:|after:|type:|fav:)/i.test(query);
}

/**
 * The paged asset list backing the current view, plus library stats.
 * Reloads whenever the view, search query, active album, or tag filter change.
 */
export function useLibraryAssets({
  view,
  query,
  activeAlbum,
  activePerson,
  activePlace,
  tagBrowse,
  refreshHistory,
  refreshExports,
  refreshDeveloperInfo,
  setTimelineAssets,
  setError,
  semanticSearchEnabled = true,
}: {
  view: View;
  query: string;
  activeAlbum: string | null;
  activePerson?: string | null;
  activePlace?: string | null;
  tagBrowse: TagBrowseFilter;
  refreshHistory: () => Promise<void>;
  refreshExports: () => Promise<void>;
  refreshDeveloperInfo: () => Promise<void>;
  setTimelineAssets: Dispatch<SetStateAction<Record<string, AssetSummary[]>>>;
  setError: Dispatch<SetStateAction<string | null>>;
  semanticSearchEnabled?: boolean;
}) {
  const [assets, setAssets] = useState<AssetSummary[]>([]);
  const [stats, setStats] = useState<LibraryStats | null>(null);
  const [smartCounts, setSmartCounts] = useState<SmartCounts | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const loadingMoreRef = useRef(false);

  const refreshStats = useCallback(async () => {
    try {
      const [libraryStats, counts] = await Promise.all([
        api.getLibraryStats(),
        api.smartCollectionCounts(),
      ]);
      setStats(libraryStats);
      setSmartCounts(counts);
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  const fetchAssetPage = useCallback(
    async (offset: number): Promise<AssetSummary[]> => {
      if (view === "trash") return api.listTrash(PAGE_SIZE, offset);
      if (view === "recent") return api.listRecent(PAGE_SIZE, offset);
      if (view === "recentViewed") {
        return api.listRecentlyViewed(PAGE_SIZE, offset);
      }
      if (view === "favorites") {
        return api.searchAssets("fav:true", PAGE_SIZE, offset);
      }
      if (isSmartCollectionKind(view)) {
        return api.listSmartCollection(view, PAGE_SIZE, offset);
      }
      if (view === "albums") {
        return activeAlbum
          ? api.listAlbumAssets(activeAlbum, PAGE_SIZE, offset)
          : [];
      }
      if (view === "people") {
        return activePerson
          ? api.listPersonAssets(activePerson, PAGE_SIZE, offset)
          : [];
      }
      if (view === "places") {
        return activePlace
          ? api.listPlaceAssets(activePlace, PAGE_SIZE, offset)
          : [];
      }
      if (view === "tags") {
        const hasFilter =
          tagBrowse.tagIds.length > 0 ||
          tagBrowse.ratings.length > 0 ||
          tagBrowse.colorLabels.length > 0;
        return hasFilter
          ? api.listTagBrowseAssets(tagBrowse, PAGE_SIZE, offset)
          : [];
      }
      if (query.trim()) {
        const q = query.trim();
        // Structured filters stay on FTS. Plain language tries CLIP first.
        if (
          semanticSearchEnabled &&
          !hasStructuredFilters(q) &&
          offset === 0
        ) {
          try {
            const semantic = await api.semanticSearch(q, PAGE_SIZE);
            if (semantic.length > 0) return semantic;
          } catch {
            // Model missing or inference failed — fall through to FTS.
          }
        }
        return api.searchAssets(q, PAGE_SIZE, offset);
      }
      return api.listAssets(PAGE_SIZE, offset);
    },
    [
      view,
      query,
      activeAlbum,
      activePerson,
      activePlace,
      tagBrowse,
      semanticSearchEnabled,
    ],
  );

  const loadAssets = useCallback(async () => {
    try {
      setError(null);
      if (view === "activity") {
        await refreshHistory();
        setAssets([]);
        await refreshStats();
        return;
      }
      if (view === "exports") {
        await refreshExports();
        setAssets([]);
        await refreshStats();
        return;
      }
      if (view === "timeline") {
        setAssets([]);
        setTimelineAssets({});
        await refreshStats();
        return;
      }
      if (view === "developer") {
        await refreshDeveloperInfo();
        setAssets([]);
        return;
      }
      if (view === "home") {
        setAssets([]);
        await refreshStats();
        return;
      }
      if (view === "watched") {
        setAssets([]);
        await refreshStats();
        return;
      }
      if (view === "savedSearches" || view === "settings") {
        setAssets([]);
        await refreshStats();
        return;
      }
      const rows = await fetchAssetPage(0);
      setAssets(rows);
      setHasMore(rows.length === PAGE_SIZE);
      await refreshStats();
    } catch (e) {
      setError(String(e));
    }
  }, [
    view,
    fetchAssetPage,
    refreshStats,
    refreshHistory,
    refreshExports,
    refreshDeveloperInfo,
    setTimelineAssets,
    setError,
  ]);

  const loadMoreAssets = useCallback(async () => {
    if (!hasMore || loadingMoreRef.current) return;
    loadingMoreRef.current = true;
    try {
      const next = await fetchAssetPage(assets.length);
      setAssets((prev) => {
        const seen = new Set(prev.map((a) => a.id));
        return [...prev, ...next.filter((a) => !seen.has(a.id))];
      });
      setHasMore(next.length === PAGE_SIZE);
    } catch (e) {
      setError(String(e));
    } finally {
      loadingMoreRef.current = false;
    }
  }, [hasMore, fetchAssetPage, assets.length, setError]);

  useEffect(() => {
    void loadAssets();
  }, [loadAssets]);

  // 30-day retention: purge expired trash rows once per app session.
  // Files on disk are never touched by the automatic purge.
  useEffect(() => {
    void api
      .purgeTrash()
      .then(async (purged) => {
        if (purged > 0) {
          setError(
            `${purged} item(s) were removed from trash after 30 days — original files on disk were kept`,
          );
          await refreshStats();
          void refreshHistory();
        }
      })
      .catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    assets,
    setAssets,
    stats,
    smartCounts,
    refreshStats,
    hasMore,
    loadAssets,
    loadMoreAssets,
  };
}
