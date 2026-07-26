import { Icon } from "./icons";

export function MediaFallback({
  type,
  compact = false,
}: {
  type: "image" | "video" | "album";
  compact?: boolean;
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
    </div>
  );
}
