import { useCallback, useEffect, useState } from "react";
import { api, type MemorySummary } from "../lib/tauri";
import type { View } from "../types/app";
import { idleDefer } from "../lib/idleDefer";

/** Memories list for Discover + Home. Reloads when the view is memories/home. */
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

  const refreshMemories = useCallback(async () => {
    try {
      setMemories(await api.listMemories(30));
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    if (view !== "memories" && view !== "home") return;
    return idleDefer(() => void refreshMemories());
  }, [view, refreshMemories]);

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

  return {
    memories,
    activeMemory,
    setActiveMemory,
    openMemoryDetail,
    refreshMemories,
    saveAsAlbum,
    saving,
  };
}
