import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import {
  api,
  type AssetSummary,
  type BlurryAsset,
  type DuplicateGroup,
} from "../lib/tauri";
import type { View } from "../types/app";

/** Duplicate groups, blurry candidates, and asset lookups for cleanup review. */
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
  const [blurry, setBlurry] = useState<BlurryAsset[]>([]);

  const loadDuplicates = useCallback(async () => {
    try {
      setError(null);
      const [groups, blurryRows] = await Promise.all([
        api.findDuplicates(),
        api.listBlurryAssets(200, 0),
      ]);
      setDupes(groups);
      setBlurry(blurryRows);
      const ids = [
        ...new Set([
          ...groups.flatMap((g) => g.assetIds),
          ...blurryRows.map((b) => b.asset.id),
        ]),
      ];
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
    for (const hit of blurry) {
      if (seen.has(hit.asset.id)) continue;
      seen.add(hit.asset.id);
      rows.push(dupeAssets.get(hit.asset.id) ?? hit.asset);
    }
    return rows;
  }, [dupes, dupeAssets, blurry]);

  return {
    dupes,
    dupeAssets,
    setDupeAssets,
    blurry,
    loadDuplicates,
    dupeAssetList,
  };
}
