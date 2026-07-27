import { useCallback, useEffect, useState } from "react";
import { api, type PlaceGroup } from "../lib/tauri";
import { idleDefer } from "../lib/idleDefer";
import type { View } from "../types/app";

export function usePlaces({
  view,
  setError,
}: {
  view: View;
  setError: (error: string | null) => void;
}) {
  const [places, setPlaces] = useState<PlaceGroup[]>([]);
  const [activePlace, setActivePlace] = useState<string | null>(null);

  const refreshPlaces = useCallback(async () => {
    try {
      setPlaces(await api.listPlaces());
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => idleDefer(() => void refreshPlaces()), [refreshPlaces]);

  useEffect(() => {
    if (view === "places") void refreshPlaces();
  }, [view, refreshPlaces]);

  return {
    places,
    activePlace,
    setActivePlace,
    refreshPlaces,
  };
}
