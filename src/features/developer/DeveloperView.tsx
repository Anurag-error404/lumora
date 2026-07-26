import { PageHeader } from "../../components/PageHeader";
import { formatBytes } from "../../lib/format";
import type { DeveloperInfo } from "../../lib/tauri";
import { ImportAnalyticsCard } from "./ImportAnalyticsCard";

/** Local diagnostics: app/database/cache details, paths, and logs. */
export function DeveloperView({
  info,
  loading,
  onRefresh,
  onOpenPath,
  onViewActivity,
}: {
  info: DeveloperInfo | null;
  loading: boolean;
  onRefresh: () => void;
  onOpenPath: (path: string, reveal?: boolean) => void;
  onViewActivity: () => void;
}) {
  return (
    <div className="developer-page">
      <PageHeader
        title="Developer"
        description="Engineering diagnostics, paths, and logs. User preferences and storage tools live under Settings."
        actions={
          <button onClick={onRefresh} disabled={loading}>
            {loading ? "Refreshing…" : "Refresh"}
          </button>
        }
      />

      {!info ? (
        <div className="developer-loading" role="status">
          <span className="spinner" aria-hidden="true" />
          Loading diagnostics…
        </div>
      ) : (
        <>
          <section className="developer-summary" aria-label="Application details">
            <article className="developer-card">
              <span className="developer-card-label">Application</span>
              <strong>LUMORA {info.appVersion}</strong>
              <dl>
                <div>
                  <dt>Build</dt>
                  <dd>{info.buildProfile}</dd>
                </div>
                <div>
                  <dt>Platform</dt>
                  <dd>
                    {info.os} · {info.arch}
                  </dd>
                </div>
                <div>
                  <dt>Debug assertions</dt>
                  <dd>{info.debugBuild ? "Enabled" : "Disabled"}</dd>
                </div>
              </dl>
            </article>

            <article className="developer-card">
              <span className="developer-card-label">Database</span>
              <strong>{formatBytes(info.databaseSizeBytes)}</strong>
              <dl>
                <div>
                  <dt>Schema</dt>
                  <dd>v{info.schemaVersion}</dd>
                </div>
                <div>
                  <dt>Activity records</dt>
                  <dd>{info.activityCount}</dd>
                </div>
                <div>
                  <dt>Export records</dt>
                  <dd>{info.exportCount}</dd>
                </div>
                <div>
                  <dt>Import runs</dt>
                  <dd>{info.importRunCount}</dd>
                </div>
                <div>
                  <dt>ffmpeg</dt>
                  <dd>{info.ffmpegAvailable ? "Available" : "Not on PATH"}</dd>
                </div>
              </dl>
              <button onClick={() => onOpenPath(info.databasePath, true)}>
                Show database
              </button>
            </article>

            <article className="developer-card">
              <span className="developer-card-label">Thumbnail cache</span>
              <strong>{formatBytes(info.thumbnailSizeBytes)}</strong>
              <dl>
                <div>
                  <dt>Files</dt>
                  <dd>{info.thumbnailCount}</dd>
                </div>
                <div>
                  <dt>Location</dt>
                  <dd className="developer-path">{info.thumbnailsPath}</dd>
                </div>
              </dl>
              <p className="muted">Clear / rebuild from Settings → Storage.</p>
            </article>

            <article className="developer-card">
              <span className="developer-card-label">Indexer</span>
              <strong>
                {info.indexProgress.running ? "Running" : "Idle"}
              </strong>
              <dl>
                <div>
                  <dt>Pending</dt>
                  <dd>{info.indexProgress.pending}</dd>
                </div>
                <div>
                  <dt>Processed</dt>
                  <dd>{info.indexProgress.processed}</dd>
                </div>
                <div>
                  <dt>Watched folders</dt>
                  <dd>{info.watchedFolderCount}</dd>
                </div>
              </dl>
              {info.indexProgress.lastPath && (
                <p
                  className="developer-path muted"
                  title={info.indexProgress.lastPath}
                >
                  Last: {info.indexProgress.lastPath}
                </p>
              )}
            </article>

            <ImportAnalyticsCard />
          </section>

          <section className="developer-paths">
            <div>
              <span className="developer-card-label">Storage paths</span>
              <code>{info.appDataPath}</code>
            </div>
            <button onClick={() => onOpenPath(info.appDataPath)}>
              Reveal data folder
            </button>
            <button onClick={() => onOpenPath(info.logsPath)}>
              Reveal logs folder
            </button>
            <button onClick={onViewActivity}>View activity log</button>
          </section>

          <section className="developer-log-section">
            <header>
              <div>
                <h3>Error & crash log</h3>
                <p className="muted">
                  Error, panic, fatal, and crash entries from the latest local
                  log.
                </p>
              </div>
              <span
                className={`developer-health ${
                  info.crashLogs.length ? "has-errors" : ""
                }`}
              >
                {info.crashLogs.length
                  ? `${info.crashLogs.length} entries`
                  : "No errors found"}
              </span>
            </header>
            {info.crashLogs.length ? (
              <pre className="developer-console developer-console-error">
                {info.crashLogs.join("\n")}
              </pre>
            ) : (
              <div className="developer-empty-log">
                No error or crash entries in the latest log file.
              </div>
            )}
          </section>

          <section className="developer-log-section">
            <header>
              <div>
                <h3>Application log</h3>
                <p className="muted">
                  Latest {info.recentLogs.length} lines · {info.logFileCount}{" "}
                  files · {formatBytes(info.logSizeBytes)}
                </p>
              </div>
              <button onClick={() => onOpenPath(info.logsPath)}>
                Open logs folder
              </button>
            </header>
            {info.recentLogs.length ? (
              <pre className="developer-console">
                {info.recentLogs.join("\n")}
              </pre>
            ) : (
              <div className="developer-empty-log">
                No application log lines available yet.
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
