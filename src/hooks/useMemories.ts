import { useCallback, useEffect, useState } from "react";
import { api, type MemorySummary } from "../lib/tauri";
import type { View } from "../types/app";
import { idleDefer } from "../lib/idleDefer";

const BUILD_POLL_MS = 1500;

/**
 * Memories list for Discover + Home.
 *
 * Reads only what the backend already grouped, so the page paints immediately.
 * While the background builder is working, `building` stays true and the list
 * is re-read every {@link BUILD_POLL_MS} until the new cards land.
 */
export function useMemories({
  view,
  setError,
}: {
  view: View;
  setError: (message: string | null) => void;
}) {
  const [memories, setMemories] = useState<MemorySummary[]>([]);
  const [activeMemory, setActiveMemory] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [building, setBuilding] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const refreshMemories = useCallback(async () => {
    try {
      const [list, status] = await Promise.all([
        api.listMemories(30),
        api.memoriesStatus(),
      ]);
      setMemories(list);
      // Never built yet reads as "still grouping", not "you have no memories".
      setBuilding(status.building || status.builtAt === null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoaded(true);
    }
  }, [setError]);

  /** Regroup in the background; the poll below picks the new cards up. */
  const rebuildMemories = useCallback(async () => {
    try {
      await api.rebuildMemories();
      setBuilding(true);
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    if (view !== "memories" && view !== "home") return;
    return idleDefer(() => void refreshMemories());
  }, [view, refreshMemories]);

  useEffect(() => {
    if (!building) return;
    if (view !== "memories" && view !== "home") return;
    const timer = setInterval(() => void refreshMemories(), BUILD_POLL_MS);
    return () => clearInterval(timer);
  }, [building, view, refreshMemories]);

  /**
   * Open immediately. Prose enrichment is fully fire-and-forget so ONNX
   * never sits on the navigation path.
   */
  const openMemoryDetail = useCallback((memoryId: string) => {
    setActiveMemory(memoryId);
    void (async () => {
      try {
        const summary = await api.enrichMemoryProse(memoryId);
        setMemories((prev) =>
          prev.map((m) => (m.id === memoryId ? summary : m)),
        );
      } catch (e) {
        console.warn("memory prose enrich failed", e);
      }
    })();
  }, []);

  const saveAsAlbum = useCallback(
    async (memoryId: string, name?: string) => {
      setSaving(true);
      try {
        const album = await api.saveMemoryAsAlbum(memoryId, name);
        return album;
      } catch (e) {
        setError(String(e));
        return null;
      } finally {
        setSaving(false);
      }
    },
    [setError],
  );

  const dismissMemory = useCallback(
    async (memoryId: string) => {
      try {
        await api.dismissMemory(memoryId);
        setMemories((prev) => prev.filter((m) => m.id !== memoryId));
        setActiveMemory((cur) => (cur === memoryId ? null : cur));
        return true;
      } catch (e) {
        setError(String(e));
        return false;
      }
    },
    [setError],
  );

  return {
    memories,
    activeMemory,
    setActiveMemory,
    openMemoryDetail,
    refreshMemories,
    rebuildMemories,
    memoriesBuilding: building,
    memoriesLoading: !loaded,
    saveAsAlbum,
    dismissMemory,
    saving,
  };
}
