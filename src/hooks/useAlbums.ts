import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type Album } from "../lib/tauri";
import { idleDefer } from "../lib/idleDefer";
import type { View } from "../types/app";

/** Album list plus the currently opened album in the Albums view. */
export function useAlbums({
  view,
  setError,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [albums, setAlbums] = useState<Album[]>([]);
  const [activeAlbum, setActiveAlbum] = useState<string | null>(null);

  const refreshAlbums = useCallback(async () => {
    try {
      setAlbums(await api.listAlbums());
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => idleDefer(() => void refreshAlbums()), [refreshAlbums]);

  useEffect(() => {
    if (view === "albums") void refreshAlbums();
  }, [view, refreshAlbums]);

  return { albums, activeAlbum, setActiveAlbum, refreshAlbums };
}
