import { useEffect, useState, type MouseEvent as ReactMouseEvent } from "react";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import {
  api,
  fileSrc,
  thumbSrc,
  type AssetSummary,
} from "../../lib/tauri";

export function AssetThumb({
  asset,
  onThumbnailUpdated,
}: {
  asset: AssetSummary;
  onThumbnailUpdated?: (asset: AssetSummary) => void;
}) {
  const [overrideSrc, setOverrideSrc] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);

  useEffect(() => {
    setOverrideSrc(null);
    setLastError(null);
  }, [asset.id, asset.thumbnailPath]);

  const src = overrideSrc ?? thumbSrc(asset);

  async function handleRetry(event: ReactMouseEvent<HTMLButtonElement>) {
    event.stopPropagation();
    event.preventDefault();
    setRetrying(true);
    setLastError(null);
    try {
      const updated = await api.regenerateAssetThumbnail(asset.id);
      if (updated.thumbnailPath) {
        const next = fileSrc(updated.thumbnailPath);
        setOverrideSrc(next);
        onThumbnailUpdated?.(updated);
      } else {
        setLastError("Preview still unavailable");
      }
    } catch (err) {
      setLastError(err instanceof Error ? err.message : String(err));
    } finally {
      setRetrying(false);
    }
  }

  return (
    <SafeImage
      src={src}
      alt=""
      loading="lazy"
      fallback={
        <MediaFallback
          type={asset.mediaType === "video" ? "video" : "image"}
          onRetry={(e) => void handleRetry(e)}
          retrying={retrying}
          errorMessage={lastError}
        />
      }
    />
  );
}
