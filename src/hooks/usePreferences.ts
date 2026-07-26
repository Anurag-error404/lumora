import {
  useCallback,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type Preferences } from "../lib/tauri";

/** Apply appearance prefs that map to CSS / document classes. */
export function applyAppearance(prefs: Preferences) {
  const root = document.documentElement;
  const size = Math.min(280, Math.max(100, prefs.appearance.thumbnailSize));
  root.style.setProperty("--thumb", `${size}px`);
  root.dataset.density = prefs.appearance.density;
  root.classList.toggle("no-animations", !prefs.appearance.animations);
  root.classList.toggle("smooth-scroll", prefs.appearance.smoothScrolling);
}

/** Load / save user preferences and keep appearance in sync with the DOM. */
export function usePreferences({
  setError,
}: {
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [prefs, setPrefs] = useState<Preferences | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const next = await api.getPreferences();
      setPrefs(next);
      applyAppearance(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [setError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const update = useCallback(
    async (mutator: (current: Preferences) => Preferences) => {
      if (!prefs) return;
      const next = mutator(structuredClone(prefs));
      setPrefs(next);
      applyAppearance(next);
      try {
        const saved = await api.setPreferences(next);
        setPrefs(saved);
        applyAppearance(saved);
      } catch (e) {
        setError(String(e));
        await refresh();
      }
    },
    [prefs, refresh, setError],
  );

  return { prefs, loading, refresh, update };
}
