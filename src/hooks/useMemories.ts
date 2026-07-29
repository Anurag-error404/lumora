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
    refreshMemories,
    saveAsAlbum,
    saving,
  };
}
