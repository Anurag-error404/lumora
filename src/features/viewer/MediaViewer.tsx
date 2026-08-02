import {
  useEffect,
  useState,
  type WheelEvent as ReactWheelEvent,
} from "react";
import { Icon } from "../../components/icons";
import { SafeImage } from "../../components/SafeImage";
import { LABEL_COLORS } from "../../lib/constants";
import { useViewerZoom } from "../../hooks/useViewerZoom";
import {
  api,
  fileSrc,
  isIdentityEditOps,
  type AssetSummary,
  type EditResult,
} from "../../lib/tauri";
import { ImageEditor } from "./ImageEditor";
import { MissingFileState } from "./MissingFileState";
import { VideoEditor } from "./VideoEditor";

/**
 * Full-screen media viewer: images render inline, videos use the platform
 * player. Pages through the surrounding grid with arrows, keys, or the wheel.
 * Supports zoom/pan for photos and videos. Images can open a basic editor
 * (rotate / crop / exposure).
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
  onRemoveFromLibrary,
  onTrash,
  isTrashView = false,
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
  onRemoveFromLibrary: (asset: AssetSummary) => void | Promise<void>;
  onTrash?: (asset: AssetSummary) => void | Promise<void>;
  isTrashView?: boolean;
}) {
  const [editing, setEditing] = useState(false);
  const [hasPendingEdits, setHasPendingEdits] = useState(false);
  const [retryKey, setRetryKey] = useState(0);
  const zoom = useViewerZoom(asset.id);
  const hasPrev = index > 0;
  const hasNext = index >= 0 && index < total - 1;
  const fileName = asset.path.split("/").pop() ?? asset.path;
  const isVideo = asset.mediaType === "video";

  useEffect(() => {
    setRetryKey(0);
  }, [asset.id]);

  useEffect(() => {
    if (isVideo) {
      setHasPendingEdits(false);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const saved = await api.getEditOps(asset.id);
        if (cancelled) return;
        setHasPendingEdits(!!saved && !isIdentityEditOps(saved.ops));
      } catch {
        if (!cancelled) setHasPendingEdits(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [asset.id, isVideo, editing]);

  useEffect(() => {
    if (editing) return;
    function onKey(e: KeyboardEvent) {
      const target = e.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
      if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        zoom.zoomIn();
      } else if (e.key === "-" || e.key === "_") {
        e.preventDefault();
        zoom.zoomOut();
      } else if (e.key === "0" && zoom.isZoomed) {
        // Prefer reset-zoom over clear-rating while zoomed.
        e.preventDefault();
        e.stopPropagation();
        zoom.reset();
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [editing, zoom.isZoomed, zoom.zoomIn, zoom.zoomOut, zoom.reset]);

  function onViewerWheel(e: ReactWheelEvent<HTMLDivElement>) {
    if (editing) return;
    zoom.onWheel(e, { onPageNext: onNext, onPagePrev: onPrev });
  }

  if (editing) {
    return (
      <div className="viewer viewer-editing" role="presentation">
        {isVideo ? (
          <VideoEditor
            asset={asset}
            onCancel={() => setEditing(false)}
            onSaved={(result) => {
              setEditing(false);
              onEdited(result);
            }}
          />
        ) : (
          <ImageEditor
            asset={asset}
            onCancel={() => setEditing(false)}
            onSaved={(result) => {
              setEditing(false);
              onEdited(result);
            }}
          />
        )}
      </div>
    );
  }

  const zoomPct = Math.round(zoom.scale * 100);

  return (
    <div
      className="viewer"
      role="dialog"
      aria-modal="true"
      aria-label={`Media viewer: ${fileName}`}
      onClick={onClose}
      onWheel={onViewerWheel}
    >
      <header className="viewer-bar" onClick={(e) => e.stopPropagation()}>
        <div className="viewer-title">
          <span className="viewer-name" title={asset.path}>
            {fileName}
          </span>
          {hasPendingEdits && (
            <span className="viewer-edited-badge" title="Has unsaved bake edits">
              Edited
            </span>
          )}
          {total > 1 && index >= 0 && (
            <span className="viewer-count">
              {index + 1} of {total}
            </span>
          )}
        </div>
        <div className="viewer-bar-actions">
          <div className="viewer-zoom-controls" role="group" aria-label="Zoom">
            <button
              type="button"
              className="viewer-icon-btn"
              title="Zoom out (−)"
              aria-label="Zoom out"
              disabled={zoom.scale <= 1}
              onClick={zoom.zoomOut}
            >
              −
            </button>
            <button
              type="button"
              className="viewer-icon-btn viewer-text-btn viewer-zoom-pct"
              title="Reset zoom (0)"
              aria-label={`Zoom ${zoomPct} percent. Click to reset`}
              onClick={zoom.reset}
            >
              {zoomPct}%
            </button>
            <button
              type="button"
              className="viewer-icon-btn"
              title="Zoom in (=)"
              aria-label="Zoom in"
              disabled={zoom.scale >= 8}
              onClick={zoom.zoomIn}
            >
              +
            </button>
          </div>
          <button
            type="button"
            className="viewer-icon-btn viewer-text-btn"
            title={isVideo ? "Edit video" : "Edit photo"}
            aria-label={isVideo ? "Edit video" : "Edit photo"}
            onClick={() => setEditing(true)}
          >
            Edit
          </button>
          {!isTrashView && onTrash && (
            <button
              type="button"
              className="viewer-icon-btn viewer-text-btn viewer-danger-btn"
              title="Move to trash"
              aria-label="Move to trash"
              onClick={() => void onTrash(asset)}
            >
              Delete
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

      <div
        className={`viewer-stage${zoom.isZoomed ? " is-zoomed" : ""}`}
        onClick={(e) => e.stopPropagation()}
        onDoubleClick={zoom.onDoubleClick}
        onPointerDown={zoom.onPointerDown}
        onPointerMove={zoom.onPointerMove}
        onPointerUp={zoom.onPointerUp}
        onPointerCancel={zoom.onPointerUp}
      >
        <div className="viewer-zoom-layer" style={zoom.transformStyle}>
          {isVideo ? (
            <ViewerVideo
              key={`${asset.id}:${retryKey}`}
              asset={asset}
              onRetry={() => setRetryKey((n) => n + 1)}
              onRemoveFromLibrary={() => void onRemoveFromLibrary(asset)}
            />
          ) : (
            <SafeImage
              key={`${asset.id}:${asset.hash}:${retryKey}`}
              src={fileSrc(asset.path)}
              alt={fileName}
              fallback={
                <MissingFileState
                  path={asset.path}
                  mediaType="image"
                  onRetry={() => setRetryKey((n) => n + 1)}
                  onRemoveFromLibrary={() => void onRemoveFromLibrary(asset)}
                />
              }
              onClick={(e) => e.stopPropagation()}
            />
          )}
        </div>
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

function ViewerVideo({
  asset,
  onRetry,
  onRemoveFromLibrary,
}: {
  asset: AssetSummary;
  onRetry: () => void;
  onRemoveFromLibrary: () => void;
}) {
  const [failed, setFailed] = useState(false);

  if (failed) {
    return (
      <MissingFileState
        path={asset.path}
        mediaType="video"
        onRetry={onRetry}
        onRemoveFromLibrary={onRemoveFromLibrary}
      />
    );
  }

  return (
    <video
      className="viewer-video"
      src={fileSrc(asset.path) ?? undefined}
      controls
      autoPlay
      playsInline
      preload="metadata"
      onClick={(e) => e.stopPropagation()}
      onError={() => setFailed(true)}
    />
  );
}
