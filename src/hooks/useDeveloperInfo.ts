import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type DeveloperInfo } from "../lib/tauri";
import type { View } from "../types/app";

/** Local diagnostics for the Developer view. */
export function useDeveloperInfo({
  view,
  setError,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [developerInfo, setDeveloperInfo] = useState<DeveloperInfo | null>(null);
  const [developerLoading, setDeveloperLoading] = useState(false);

  const refreshDeveloperInfo = useCallback(async () => {
    setDeveloperLoading(true);
    try {
      setDeveloperInfo(await api.getDeveloperInfo());
    } catch (e) {
      setError(String(e));
    } finally {
      setDeveloperLoading(false);
    }
  }, [setError]);

  useEffect(() => {
    if (view === "developer" || view === "settings") void refreshDeveloperInfo();
  }, [view, refreshDeveloperInfo]);

  return { developerInfo, developerLoading, refreshDeveloperInfo };
}
