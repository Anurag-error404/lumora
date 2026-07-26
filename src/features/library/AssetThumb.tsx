import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { thumbSrc, type AssetSummary } from "../../lib/tauri";

export function AssetThumb({ asset }: { asset: AssetSummary }) {
  const src = thumbSrc(asset);
  return (
    <SafeImage
      src={src}
      alt=""
      loading="lazy"
      fallback={
        <MediaFallback
          type={asset.mediaType === "video" ? "video" : "image"}
        />
      }
    />
  );
}
