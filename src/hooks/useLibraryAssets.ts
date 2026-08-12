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
import { mergeSearchResults } from "../features/search/merge-search-results";

/** Smaller first paint for memory detail (CLIP diversity is O(n²)). */
const MEMORY_PAGE_SIZE = 72;
/** Settling time before FTS/CLIP search runs — typing must not fire per key. */
const SEARCH_DEBOUNCE_MS = 350;
/** CLIP text embed is wasted on 1–2 character stubs. */
const SEMANTIC_MIN_CHARS = 3;

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
  activeMemory,
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
  activeMemory?: string | null;
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
  // Clear immediately; debounce non-empty typing so each keystroke isn't an IPC.
  const [searchQuery, setSearchQuery] = useState(query);
  useEffect(() => {
    if (!query.trim()) {
      setSearchQuery("");
      return;
    }
    const id = window.setTimeout(() => setSearchQuery(query), SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(id);
  }, [query]);

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
      if (view === "memories") {
        return activeMemory
          ? api.listMemoryAssets(activeMemory, MEMORY_PAGE_SIZE, offset)
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
      if (searchQuery.trim()) {
        const q = searchQuery.trim();
        // Structured filters stay on FTS. Plain language blends CLIP recall with
        // FTS (filename / tags / OCR text / people / auto-tags) so a keyword that
        // appears in an image is never dropped just because CLIP returned hits.
        if (
          semanticSearchEnabled &&
          !hasStructuredFilters(q) &&
          q.length >= SEMANTIC_MIN_CHARS &&
          offset === 0
        ) {
          const ftsPromise = api.searchAssets(q, PAGE_SIZE, 0);
          let semantic: AssetSummary[] = [];
          try {
            semantic = await api.semanticSearch(q, PAGE_SIZE);
          } catch {
            // Model missing or inference failed — FTS alone is still useful.
          }
          const fts = await ftsPromise;
          const merged = mergeSearchResults(fts, semantic, PAGE_SIZE);
          if (merged.length > 0) return merged;
        }
        return api.searchAssets(q, PAGE_SIZE, offset);
      }
      return api.listAssets(PAGE_SIZE, offset);
    },
    [
      view,
      searchQuery,
      activeAlbum,
      activePerson,
      activePlace,
      activeMemory,
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
      const pageSize = view === "memories" ? MEMORY_PAGE_SIZE : PAGE_SIZE;
      setHasMore(rows.length === pageSize);
      // Don't block memory detail paint on stats refresh.
      if (view === "memories" && activeMemory) {
        void refreshStats();
      } else {
        await refreshStats();
      }
    } catch (e) {
      setError(String(e));
    }
  }, [
    view,
    activeMemory,
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
      const pageSize = view === "memories" ? MEMORY_PAGE_SIZE : PAGE_SIZE;
      setHasMore(next.length === pageSize);
    } catch (e) {
      setError(String(e));
    } finally {
      loadingMoreRef.current = false;
    }
  }, [hasMore, fetchAssetPage, assets.length, setError, view]);

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
