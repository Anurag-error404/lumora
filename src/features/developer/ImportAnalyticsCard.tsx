import { useEffect, useState } from "react";
import { api, type ImportRun } from "../../lib/tauri";

/** Compact local-only import performance history for Developer. */
export function ImportAnalyticsCard() {
  const [runs, setRuns] = useState<ImportRun[]>([]);

  useEffect(() => {
    void api
      .listImportRuns(8)
      .then(setRuns)
      .catch(() => setRuns([]));
  }, []);

  if (!runs.length) {
    return (
      <article className="developer-card">
        <span className="developer-card-label">Import performance</span>
        <strong>No runs yet</strong>
        <p className="muted">
          Each import records duration and files/sec locally. Nothing is sent
          off-device.
        </p>
      </article>
    );
  }

  return (
    <article className="developer-card">
      <span className="developer-card-label">Import performance</span>
      <strong>
        {runs[0].filesPerSec != null
          ? `${runs[0].filesPerSec.toFixed(1)} files/s last`
          : `${(runs[0].durationMs / 1000).toFixed(1)}s last`}
      </strong>
      <ul className="developer-import-runs">
        {runs.map((run) => (
          <li key={run.id}>
            <span>
              {run.scanned} files · {(run.durationMs / 1000).toFixed(1)}s
              {run.filesPerSec != null
                ? ` · ${run.filesPerSec.toFixed(1)}/s`
                : ""}
              {run.cancelled ? " · stopped" : ""}
            </span>
            <span className="muted">{run.finishedAt.slice(0, 19)}</span>
          </li>
        ))}
      </ul>
    </article>
  );
}
