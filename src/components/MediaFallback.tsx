import type { MouseEvent as ReactMouseEvent } from "react";
import { Icon } from "./icons";

export function MediaFallback({
  type,
  compact = false,
  onRetry,
  retrying = false,
  errorMessage,
}: {
  type: "image" | "video" | "album";
  compact?: boolean;
  onRetry?: (event: ReactMouseEvent<HTMLButtonElement>) => void;
  retrying?: boolean;
  errorMessage?: string | null;
}) {
  const icon =
    type === "album" ? "album" : type === "video" ? "camera" : "library";
  const label =
    type === "album"
      ? "No album cover"
      : type === "video"
        ? "Video preview unavailable"
        : "Preview unavailable";
  return (
    <div className={`media-fallback ${compact ? "compact" : ""}`}>
      <span className="media-fallback-icon" aria-hidden="true">
        <Icon name={icon} />
      </span>
      {!compact && <span>{label}</span>}
      {errorMessage && !compact && (
        <span className="media-fallback-error" title={errorMessage}>
          {errorMessage}
        </span>
      )}
      {onRetry && (
        <button
          type="button"
          className="media-fallback-retry"
          disabled={retrying}
          onClick={onRetry}
          onPointerDown={(e) => e.stopPropagation()}
        >
          {retrying ? "Retrying…" : "Retry"}
        </button>
      )}
    </div>
  );
}
