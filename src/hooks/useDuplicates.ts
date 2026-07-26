import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { api, type AssetSummary, type DuplicateGroup } from "../lib/tauri";
import type { View } from "../types/app";

/** Duplicate groups plus a lookup map of the assets they reference. */
export function useDuplicates({
  view,
  setError,
}: {
  view: View;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [dupes, setDupes] = useState<DuplicateGroup[]>([]);
  const [dupeAssets, setDupeAssets] = useState<Map<string, AssetSummary>>(
    new Map(),
  );

  const loadDuplicates = useCallback(async () => {
    try {
      setError(null);
      const groups = await api.findDuplicates();
      setDupes(groups);
      const ids = [...new Set(groups.flatMap((g) => g.assetIds))];
      const rows = ids.length ? await api.listAssetsByIds(ids) : [];
      setDupeAssets(new Map(rows.map((a) => [a.id, a])));
    } catch (e) {
      setError(String(e));
    }
  }, [setError]);

  useEffect(() => {
    if (view === "duplicates") void loadDuplicates();
  }, [view, loadDuplicates]);

  const dupeAssetList = useMemo(() => {
    const seen = new Set<string>();
    const rows: AssetSummary[] = [];
    for (const group of dupes) {
      for (const id of group.assetIds) {
        if (seen.has(id)) continue;
        const asset = dupeAssets.get(id);
        if (!asset) continue;
        seen.add(id);
        rows.push(asset);
      }
    }
    return rows;
  }, [dupes, dupeAssets]);

  return { dupes, dupeAssets, setDupeAssets, loadDuplicates, dupeAssetList };
}
