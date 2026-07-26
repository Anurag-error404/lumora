import type { ImportProgressEvent } from "../../lib/tauri";

/** Progress strip shown under the toolbar while an import runs. */
export function ImportProgressBar({
  progress,
  pct,
}: {
  progress: ImportProgressEvent;
  pct: number | null;
}) {
  return (
    <div className="import-progress" role="status" aria-live="polite">
      <div className="import-progress-meta">
        <span className="import-progress-label">
          <span className="spinner" aria-hidden="true" />
          {progress.phase === "scanning"
            ? "Scanning for media…"
            : progress.phase === "done"
              ? "Import complete"
              : `Importing ${progress.current} / ${progress.total}`}
        </span>
        {pct !== null && <span>{pct}%</span>}
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
                    progress.phase === "done"
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
