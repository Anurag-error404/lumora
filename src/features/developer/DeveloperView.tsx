import { useCallback, useEffect, useState } from "react";
import { PageHeader } from "../../components/PageHeader";
import { formatBytes } from "../../lib/format";
import { api, type DeveloperInfo, type PluginEntry, type PluginRunRecord } from "../../lib/tauri";
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

          <PluginDiagnosticsSection />
        </>
      )}
    </div>
  );
}

const OUTCOME_COLORS: Record<string, string> = {
  ok: "var(--color-success, #27ae60)",
  cancelled: "var(--color-muted, #999)",
  timeout: "var(--color-warning, #e67e22)",
  error: "var(--color-error, #c0392b)",
};

const OUTCOME_LABEL: Record<string, string> = {
  ok: "✓ OK",
  cancelled: "↩ Cancelled",
  timeout: "⏱ Timeout",
  error: "✗ Error",
};

function PluginDiagnosticsSection() {
  const [plugins, setPlugins] = useState<PluginEntry[]>([]);
  const [records, setRecords] = useState<PluginRunRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [clearBusy, setClearBusy] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const installed = await api.listPlugins();
      setPlugins(installed);
      const tail: PluginRunRecord[] = [];
      for (const p of installed) {
        const recs = await api.getPluginHistory(p.manifest.id, 50);
        tail.push(...recs);
      }
      // Sort newest first across all plugins.
      tail.sort((a, b) => b.startedAt.localeCompare(a.startedAt));
      setRecords(tail.slice(0, 50));
    } catch {
      /* silently ignore; this is a diagnostics panel */
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const handleClearAll = async () => {
    if (!window.confirm("Clear plugin run history for all plugins?")) return;
    setClearBusy(true);
    try {
      await api.clearAllPluginHistory();
      setRecords([]);
    } catch {
      /* ignore */
    } finally {
      setClearBusy(false);
    }
  };

  if (plugins.length === 0 && !loading) return null;

  return (
    <section className="developer-log-section">
      <header>
        <div>
          <h3>Plugin run history</h3>
          <p className="muted">
            Last 50 runs across {plugins.length} installed plugin(s).
          </p>
        </div>
        <button disabled={clearBusy || loading} onClick={() => void handleClearAll()}>
          Clear all history
        </button>
      </header>
      {loading ? (
        <div className="developer-loading" role="status">
          <span className="spinner" aria-hidden="true" />
          Loading plugin history…
        </div>
      ) : records.length === 0 ? (
        <div className="developer-empty-log">No plugin runs recorded yet.</div>
      ) : (
        <ul className="developer-plugin-history">
          {records.map((rec) => (
            <PluginRunRow key={rec.runId} record={rec} />
          ))}
        </ul>
      )}
    </section>
  );
}

function PluginRunRow({ record }: { record: PluginRunRecord }) {
  const [open, setOpen] = useState(false);
  return (
    <li className="developer-plugin-run">
      <button
        type="button"
        className="developer-plugin-run-summary"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span style={{ color: OUTCOME_COLORS[record.outcome], minWidth: "60px" }}>
          {OUTCOME_LABEL[record.outcome] ?? record.outcome}
        </span>
        <span className="muted" style={{ flex: 1, textAlign: "left", fontSize: "12px" }}>
          {record.pluginId} · {record.actionId} · {record.mode} ·{" "}
          {new Date(record.startedAt).toLocaleString()} · {record.durationMs}ms ·{" "}
          {record.assetsAffected}/{record.assetsRequested} assets
        </span>
        <span aria-hidden="true">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <div className="developer-plugin-run-detail">
          {record.errorMessage && (
            <p style={{ color: "var(--color-error, #c0392b)", fontSize: "12px", marginBottom: "4px" }}>
              <strong>{record.errorCode}</strong>: {record.errorMessage}
            </p>
          )}
          {record.logLines.length > 0 ? (
            <pre className="developer-console" style={{ maxHeight: "200px", overflow: "auto" }}>
              {record.logLines
                .map((l) => `[+${l.timestampMs}ms] [${l.level}] ${l.message}`)
                .join("\n")}
            </pre>
          ) : (
            <p className="muted" style={{ fontSize: "11px" }}>No log lines.</p>
          )}
        </div>
      )}
    </li>
  );
}
