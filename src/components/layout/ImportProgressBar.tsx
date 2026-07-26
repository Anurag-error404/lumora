import type { ImportProgressEvent } from "../../lib/tauri";

/** Progress strip shown under the toolbar while an import runs. */
export function ImportProgressBar({
  progress,
  pct,
  onCancel,
}: {
  progress: ImportProgressEvent;
  pct: number | null;
  onCancel?: () => void;
}) {
  const canCancel =
    !!onCancel &&
    (progress.phase === "scanning" || progress.phase === "indexing");

  return (
    <div className="import-progress" role="status" aria-live="polite">
      <div className="import-progress-meta">
        <span className="import-progress-label">
          <span className="spinner" aria-hidden="true" />
          {progress.phase === "scanning"
            ? "Scanning for media…"
            : progress.phase === "done"
              ? "Import complete"
              : progress.phase === "cancelled"
                ? "Import cancelled"
                : `Importing ${progress.current} / ${progress.total}`}
        </span>
        <span className="import-progress-actions">
          {pct !== null && progress.phase === "indexing" && <span>{pct}%</span>}
          {canCancel && (
            <button type="button" className="import-cancel" onClick={onCancel}>
              Stop
            </button>
          )}
        </span>
      </div>
      <div
        className={`import-progress-track ${
          progress.phase === "scanning" ? "indeterminate" : ""
        }`}
      >
        <div
          className="import-progress-fill"
          style={
            progress.phase === "scanning"
              ? undefined
              : {
                  width:
                    progress.phase === "done" || progress.phase === "cancelled"
                      ? "100%"
                      : pct !== null
                        ? `${pct}%`
                        : "0%",
                }
          }
        />
      </div>
      {progress.phase === "indexing" && (
        <p className="muted import-progress-file">{progress.path}</p>
      )}
    </div>
  );
}
