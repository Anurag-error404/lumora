import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type AssetSummary } from "../lib/tauri";

/**
 * Lightbox + media-info state. The viewer pages through whichever grid the
 * opened asset came from (library page, timeline feed, or duplicate list).
 */
export function useViewer({
  assets,
  timelineAssets,
  timelineFlatAssets,
  dupeAssets,
  dupeAssetList,
  homeRecent,
  loadMoreAssets,
}: {
  assets: AssetSummary[];
  timelineAssets: Record<string, AssetSummary[]>;
  timelineFlatAssets: AssetSummary[];
  dupeAssets: Map<string, AssetSummary>;
  dupeAssetList: AssetSummary[];
  homeRecent: AssetSummary[];
  loadMoreAssets: () => Promise<void>;
}) {
  const [lightboxId, setLightboxId] = useState<string | null>(null);
  const [infoAssetId, setInfoAssetId] = useState<string | null>(null);

  const lightboxAsset = useMemo(() => {
    if (!lightboxId) return null;
    return (
      assets.find((a) => a.id === lightboxId) ??
      Object.values(timelineAssets)
        .flat()
        .find((a) => a.id === lightboxId) ??
      dupeAssets.get(lightboxId) ??
      homeRecent.find((a) => a.id === lightboxId) ??
      null
    );
  }, [assets, timelineAssets, dupeAssets, homeRecent, lightboxId]);

  const infoAsset = useMemo(() => {
    if (!infoAssetId) return null;
    return (
      assets.find((a) => a.id === infoAssetId) ??
      Object.values(timelineAssets)
        .flat()
        .find((a) => a.id === infoAssetId) ??
      dupeAssets.get(infoAssetId) ??
      homeRecent.find((a) => a.id === infoAssetId) ??
      null
    );
  }, [assets, timelineAssets, dupeAssets, homeRecent, infoAssetId]);

  /** Ordered list the viewer pages through, matching whichever grid was opened. */
  const viewerList = useMemo<AssetSummary[]>(() => {
    if (!lightboxId) return [];
    if (assets.some((a) => a.id === lightboxId)) return assets;
    if (timelineFlatAssets.some((a) => a.id === lightboxId)) {
      return timelineFlatAssets;
    }
    if (dupeAssetList.some((a) => a.id === lightboxId)) return dupeAssetList;
    if (homeRecent.some((a) => a.id === lightboxId)) return homeRecent;
    return lightboxAsset ? [lightboxAsset] : [];
  }, [lightboxId, assets, timelineFlatAssets, dupeAssetList, homeRecent, lightboxAsset]);

  const viewerIndex = useMemo(
    () => viewerList.findIndex((a) => a.id === lightboxId),
    [viewerList, lightboxId],
  );

  const showPrevMedia = useCallback(() => {
    if (viewerIndex > 0) setLightboxId(viewerList[viewerIndex - 1].id);
  }, [viewerIndex, viewerList]);

  const showNextMedia = useCallback(() => {
    if (viewerIndex < 0) return;
    if (viewerIndex < viewerList.length - 1) {
      setLightboxId(viewerList[viewerIndex + 1].id);
    }
    // Keep paging fluid at the tail of a virtualised library page.
    if (viewerList === assets && viewerIndex >= viewerList.length - 3) {
      void loadMoreAssets();
    }
  }, [viewerIndex, viewerList, assets, loadMoreAssets]);

  // Track recently viewed whenever the open asset changes.
  useEffect(() => {
    if (!lightboxId) return;
    void api.recordAssetView(lightboxId).catch(() => undefined);
  }, [lightboxId]);

  return {
    lightboxId,
    setLightboxId,
    infoAssetId,
    setInfoAssetId,
    lightboxAsset,
    infoAsset,
    viewerList,
    viewerIndex,
    showPrevMedia,
    showNextMedia,
  };
}
