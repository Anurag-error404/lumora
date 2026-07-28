import { Icon } from "../../components/icons";

/** Shown when the original file cannot be loaded from disk. */
export function MissingFileState({
  path,
  mediaType,
  onRetry,
  onRemoveFromLibrary,
}: {
  path: string;
  mediaType: "image" | "video";
  onRetry: () => void;
  onRemoveFromLibrary: () => void;
}) {
  return (
    <div
      className="missing-file-state"
      role="alert"
      onClick={(e) => e.stopPropagation()}
    >
      <span className="missing-file-icon" aria-hidden="true">
        <Icon name={mediaType === "video" ? "camera" : "library"} />
      </span>
      <h2 className="missing-file-title">File not found</h2>
      <p className="missing-file-copy">
        This item is in your library, but the file is missing at the path below.
        It may have been moved, renamed, or the drive may be disconnected.
      </p>
      <p className="missing-file-path" title={path}>
        {path}
      </p>
      <div className="missing-file-actions">
        <button type="button" className="primary" onClick={onRetry}>
          Retry
        </button>
        <button type="button" className="danger" onClick={onRemoveFromLibrary}>
          Remove from Library
        </button>
      </div>
    </div>
  );
}
