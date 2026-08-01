import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  api,
  type AssetSummary,
  type BlurryAsset,
  type DuplicateGroup,
  type DuplicateScanProgress,
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
  const [scanning, setScanning] = useState(false);
  const [scanMessage, setScanMessage] = useState<string | null>(null);
  const [scanProgress, setScanProgress] =
    useState<DuplicateScanProgress | null>(null);

  const hydrateAssets = useCallback(async (groups: DuplicateGroup[], blurryRows: BlurryAsset[]) => {
    const ids = [
      ...new Set([
        ...groups.flatMap((g) => g.assetIds),
        ...blurryRows.map((b) => b.asset.id),
      ]),
    ];
    const rows = ids.length ? await api.listAssetsByIds(ids) : [];
    setDupeAssets(new Map(rows.map((a) => [a.id, a])));
  }, []);

  const loadDuplicates = useCallback(async () => {
    try {
      setError(null);
      const [groups, blurryRows] = await Promise.all([
        api.findDuplicates(),
        api.listBlurryAssets(200, 0),
      ]);
      setDupes(groups);
      setBlurry(blurryRows);
      await hydrateAssets(groups, blurryRows);
    } catch (e) {
      setError(String(e));
    }
  }, [hydrateAssets, setError]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<DuplicateScanProgress>("duplicate-scan-progress", (event) => {
      setScanProgress(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  /** Backfill missing phash/blur, then regroup exact + near duplicates. */
  const scanDuplicates = useCallback(async () => {
    setScanning(true);
    setScanMessage(null);
    setScanProgress({ phase: "phash", current: 0, total: 0, path: null });
    try {
      setError(null);
      const result = await api.scanDuplicates(2000);
      const blurryRows = await api.listBlurryAssets(200, 0);
      setDupes(result.groups);
      setBlurry(blurryRows);
      await hydrateAssets(result.groups, blurryRows);
      const parts: string[] = [
        `${result.exactGroups} exact group${result.exactGroups === 1 ? "" : "s"}`,
        `${result.nearGroups} near group${result.nearGroups === 1 ? "" : "s"}`,
      ];
      if (result.copiesIndexed > 0) {
        parts.push(`${result.copiesIndexed} copies indexed`);
      }
      if (result.phashBackfilled > 0) {
        parts.push(`${result.phashBackfilled} hashes filled`);
      }
      if (result.blurScored > 0) {
        parts.push(`${result.blurScored} blur scores`);
      }
      setScanMessage(parts.join(" · "));
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
      setScanProgress(null);
    }
  }, [hydrateAssets, setError]);

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
    scanDuplicates,
    scanning,
    scanMessage,
    scanProgress,
    dupeAssetList,
  };
}
