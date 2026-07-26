import { useRef, useState, type WheelEvent as ReactWheelEvent } from "react";
import { Icon } from "../../components/icons";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { LABEL_COLORS } from "../../lib/constants";
import { fileSrc, type AssetSummary, type EditResult } from "../../lib/tauri";
import { ImageEditor } from "./ImageEditor";

/**
 * Full-screen media viewer: images render inline, videos use the platform
 * player. Pages through the surrounding grid with arrows, keys, or the wheel.
 * Images can open a basic editor (rotate / crop / exposure).
 */
export function MediaViewer({
  asset,
  index,
  total,
  onClose,
  onPrev,
  onNext,
  onRate,
  onLabel,
  onToggleFavorite,
  onShowInfo,
  onEdited,
}: {
  asset: AssetSummary;
  index: number;
  total: number;
  onClose: () => void;
  onPrev: () => void;
  onNext: () => void;
  onRate: (asset: AssetSummary, rating: number) => void | Promise<void>;
  onLabel: (
    asset: AssetSummary,
    colorLabel: string | null,
  ) => void | Promise<void>;
  onToggleFavorite: (asset: AssetSummary) => void | Promise<void>;
  onShowInfo: () => void;
  onEdited: (result: EditResult) => void;
}) {
  const wheelLockRef = useRef(0);
  const [editing, setEditing] = useState(false);
  const hasPrev = index > 0;
  const hasNext = index >= 0 && index < total - 1;
  const fileName = asset.path.split("/").pop() ?? asset.path;
  const isVideo = asset.mediaType === "video";

  function onWheel(e: ReactWheelEvent<HTMLDivElement>) {
    if (editing) return;
    const delta =
      Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
    if (Math.abs(delta) < 12) return;
    const now = Date.now();
    if (now - wheelLockRef.current < 260) return;
    wheelLockRef.current = now;
    if (delta > 0) onNext();
    else onPrev();
  }

  if (editing && !isVideo) {
    return (
      <div className="viewer viewer-editing" role="presentation">
        <ImageEditor
          asset={asset}
          onCancel={() => setEditing(false)}
          onSaved={(result) => {
            setEditing(false);
            onEdited(result);
          }}
        />
      </div>
    );
  }

  return (
    <div
      className="viewer"
      role="dialog"
      aria-modal="true"
      aria-label={`Media viewer: ${fileName}`}
      onClick={onClose}
      onWheel={onWheel}
    >
      <header className="viewer-bar" onClick={(e) => e.stopPropagation()}>
        <div className="viewer-title">
          <span className="viewer-name" title={asset.path}>
            {fileName}
          </span>
          {total > 1 && index >= 0 && (
            <span className="viewer-count">
              {index + 1} of {total}
            </span>
          )}
        </div>
        <div className="viewer-bar-actions">
          {!isVideo && (
            <button
              type="button"
              className="viewer-icon-btn viewer-text-btn"
              title="Edit photo"
              aria-label="Edit photo"
              onClick={() => setEditing(true)}
            >
              Edit
            </button>
          )}
          <button
            type="button"
            className="viewer-icon-btn"
            title="Media information"
            aria-label="Show media information"
            onClick={onShowInfo}
          >
            <Icon name="info" />
          </button>
          <button
            type="button"
            className="viewer-icon-btn"
            title="Close viewer (Esc)"
            aria-label="Close viewer"
            onClick={onClose}
          >
            <Icon name="close" />
          </button>
        </div>
      </header>

      <button
        type="button"
        className="viewer-nav prev"
        title="Previous (←)"
        aria-label="Previous item"
        disabled={!hasPrev}
        onClick={(e) => {
          e.stopPropagation();
          onPrev();
        }}
      >
        <Icon name="chevronLeft" />
      </button>

      <div className="viewer-stage">
        {isVideo ? (
          <ViewerVideo key={asset.id} asset={asset} />
        ) : (
          <SafeImage
            key={`${asset.id}:${asset.hash}:${asset.thumbnailPath ?? ""}`}
            src={fileSrc(asset.path)}
            alt={fileName}
            fallback={
              <div
                className="viewer-fallback"
                onClick={(e) => e.stopPropagation()}
              >
                <MediaFallback type="image" />
              </div>
            }
            onClick={(e) => e.stopPropagation()}
          />
        )}
      </div>

      <button
        type="button"
        className="viewer-nav next"
        title="Next (→)"
        aria-label="Next item"
        disabled={!hasNext}
        onClick={(e) => {
          e.stopPropagation();
          onNext();
        }}
      >
        <Icon name="chevronRight" />
      </button>

      <div className="viewer-meta" onClick={(e) => e.stopPropagation()}>
        <span className="viewer-stars" role="group" aria-label="Rate this item">
          {[1, 2, 3, 4, 5].map((n) => (
            <button
              key={n}
              type="button"
              className={`lightbox-star ${n <= asset.rating ? "on" : ""}`}
              title={
                n === asset.rating
                  ? "Clear rating"
                  : `Rate ${n} star${n > 1 ? "s" : ""}`
              }
              aria-label={`Rate ${n} star${n > 1 ? "s" : ""}`}
              onClick={() => void onRate(asset, n === asset.rating ? 0 : n)}
            >
              ★
            </button>
          ))}
        </span>
        <span
          className="lightbox-swatches"
          role="group"
          aria-label="Colour label"
        >
          {LABEL_COLORS.map((color) => (
            <button
              key={color.id}
              type="button"
              className={`swatch ${
                asset.colorLabel === color.id ? "on" : ""
              }`}
              style={{ background: color.hex }}
              title={
                asset.colorLabel === color.id
                  ? "Remove label"
                  : `Label ${color.id}`
              }
              aria-label={`Label ${color.id}`}
              onClick={() =>
                void onLabel(
                  asset,
                  asset.colorLabel === color.id ? null : color.id,
                )
              }
            />
          ))}
        </span>
        <button
          type="button"
          className={`fav-btn inline ${asset.favorite ? "on" : ""}`}
          onClick={() => void onToggleFavorite(asset)}
        >
          <Icon name={asset.favorite ? "heart" : "heartOutline"} />
          {asset.favorite ? "Favourited" : "Favourite"}
        </button>
      </div>
    </div>
  );
}

function ViewerVideo({ asset }: { asset: AssetSummary }) {
  const [failed, setFailed] = useState(false);

  if (failed) {
    return (
      <div className="viewer-fallback" onClick={(e) => e.stopPropagation()}>
        <MediaFallback type="video" />
      </div>
    );
  }

  return (
    <video
      className="viewer-video"
      src={fileSrc(asset.path)}
      controls
      autoPlay
      playsInline
      preload="metadata"
      onClick={(e) => e.stopPropagation()}
      onError={() => setFailed(true)}
    />
  );
}
