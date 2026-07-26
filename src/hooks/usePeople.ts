import { useCallback, useEffect, useState } from "react";
import { api, type Person } from "../lib/tauri";
import type { View } from "../types/app";

export function usePeople({
  view,
  setError,
}: {
  view: View;
  setError: (error: string | null) => void;
}) {
  const [people, setPeople] = useState<Person[]>([]);
  const [ignoredPeople, setIgnoredPeople] = useState<Person[]>([]);
  const [activePerson, setActivePerson] = useState<string | null>(null);

  const refreshPeople = useCallback(async () => {
    try {
      const [visible, ignored] = await Promise.all([
        api.listPeople(),
        api.listIgnoredPeople(),
      ]);
      setPeople(visible);
      setIgnoredPeople(ignored);
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  const setPersonIgnored = useCallback(
    async (personId: string, ignored: boolean) => {
      try {
        await api.setPersonIgnored(personId, ignored);
        setActivePerson((current) =>
          ignored && current === personId ? null : current,
        );
        await refreshPeople();
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshPeople, setError],
  );

  useEffect(() => {
    void refreshPeople();
  }, [refreshPeople]);

  useEffect(() => {
    if (view === "people") void refreshPeople();
  }, [view, refreshPeople]);

  return {
    people,
    ignoredPeople,
    activePerson,
    setActivePerson,
    refreshPeople,
    setPersonIgnored,
  };
}
