import { useEffect, useRef } from "react";
import type { PluginRunProgressEvent } from "../../lib/tauri";

export function PluginRunDialog({
  progress,
  pct,
  onClose,
}: {
  progress: PluginRunProgressEvent;
  pct: number | null;
  onClose: () => void;
}) {
  const logRef = useRef<HTMLDivElement>(null);
  const isDone = progress.phase === "done" || progress.phase === "error";
  const isRunning = progress.phase === "starting" || progress.phase === "running";

  useEffect(() => {
    const el = logRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [progress.logs.length]);

  const statusLabel =
    progress.phase === "starting"
      ? "Starting plugin…"
      : progress.phase === "running"
        ? progress.total > 0
          ? `Processing ${progress.current} / ${progress.total}`
          : "Running…"
        : progress.phase === "done"
          ? "Completed"
          : "Failed";

  return (
    <div className="modal-backdrop" onClick={isDone ? onClose : undefined}>
      <div
        className="modal plugin-run-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="plugin-run-title"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="plugin-run-header">
          <div>
            <h2 id="plugin-run-title">{progress.pluginName}</h2>
            <p className="muted plugin-run-subtitle">{progress.actionId}</p>
          </div>
          {isRunning && <span className="spinner" aria-hidden="true" />}
        </header>

        <div className="plugin-run-status" role="status" aria-live="polite">
          <span className={`plugin-run-phase plugin-run-phase--${progress.phase}`}>
            {statusLabel}
          </span>
          {pct !== null && isRunning && <span className="plugin-run-pct">{pct}%</span>}
        </div>

        <div
          className={`import-progress-track ${progress.phase === "starting" ? "indeterminate" : ""}`}
        >
          <div
            className="import-progress-fill"
            style={
              progress.phase === "starting"
                ? undefined
                : {
                    width:
                      progress.phase === "done"
                        ? "100%"
                        : progress.phase === "error"
                          ? "100%"
                          : pct !== null
                            ? `${pct}%`
                            : "0%",
                  }
            }
          />
        </div>

        {progress.message && (
          <p className={`plugin-run-message ${progress.phase === "error" ? "is-error" : ""}`}>
            {progress.message}
          </p>
        )}

        <div className="plugin-run-logs" ref={logRef}>
          {progress.logs.length === 0 ? (
            <p className="muted plugin-run-logs-empty">Waiting for plugin output…</p>
          ) : (
            <ul>
              {progress.logs.map((line, i) => (
                <li key={`${line.timestampMs}-${i}`} className={`plugin-log-line log-${line.level}`}>
                  <span className="log-ts">+{line.timestampMs}ms</span>
                  <span className={`log-level log-level-${line.level}`}>{line.level}</span>
                  <span className="log-msg">{line.message}</span>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="modal-actions">
          {isDone ? (
            <button type="button" className="primary" onClick={onClose}>
              Close
            </button>
          ) : (
            <button type="button" disabled>
              Running…
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
