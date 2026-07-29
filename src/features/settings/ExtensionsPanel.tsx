import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type PluginEntry, type PluginRunRecord } from "../../lib/tauri";
import { SettingsBlock } from "./settingsUi";

const PERMISSION_LABELS: Record<string, string> = {
  "read:assets": "Read Assets",
  "read:metadata": "Read Metadata",
  "write:metadata": "Write Metadata",
  "rename:filesystem": "Rename Files",
  "move:filesystem": "Move Files",
  "copy:filesystem": "Copy Files",
  "delete:filesystem": "Delete Files",
  "export:assets": "Export Assets",
};

const OUTCOME_BADGE: Record<string, string> = {
  ok: "✓ OK",
  cancelled: "↩ Cancelled",
  timeout: "⏱ Timeout",
  error: "✗ Error",
};

export function ExtensionsPanel() {
  const [plugins, setPlugins] = useState<PluginEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [history, setHistory] = useState<Record<string, PluginRunRecord[]>>({});
  const [historyLoading, setHistoryLoading] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setPlugins(await api.listPlugins());
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggleEnabled = async (plugin: PluginEntry) => {
    setBusy(plugin.manifest.id);
    try {
      await api.setPluginEnabled(plugin.manifest.id, !plugin.enabled);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const addPlugin = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Select plugin folder" });
    if (!dir || typeof dir !== "string") return;
    setBusy("install");
    try {
      await api.installPluginDir(dir);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const removePlugin = async (plugin: PluginEntry) => {
    if (
      !window.confirm(
        `Remove "${plugin.manifest.name}"?\n\nThe plugin folder and its history will be permanently deleted.`,
      )
    )
      return;
    setBusy(plugin.manifest.id);
    try {
      await api.removePlugin(plugin.manifest.id);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const toggleHistory = async (plugin: PluginEntry) => {
    const id = plugin.manifest.id;
    if (expanded === id) {
      setExpanded(null);
      return;
    }
    setExpanded(id);
    if (!history[id]) {
      setHistoryLoading(id);
      try {
        const records = await api.getPluginHistory(id, 20);
        setHistory((prev) => ({ ...prev, [id]: records }));
      } catch (e) {
        setError(String(e));
      } finally {
        setHistoryLoading(null);
      }
    }
  };

  const clearHistory = async (plugin: PluginEntry) => {
    setBusy(plugin.manifest.id + ":history");
    try {
      await api.clearPluginHistory(plugin.manifest.id);
      setHistory((prev) => {
        const next = { ...prev };
        delete next[plugin.manifest.id];
        return next;
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  if (loading) {
    return (
      <div className="developer-loading" role="status">
        <span className="spinner" aria-hidden="true" />
        Loading extensions…
      </div>
    );
  }

  return (
    <>
      {error && <p className="muted" style={{ color: "var(--color-error, #c0392b)" }}>{error}</p>}

      <SettingsBlock title="Installed extensions">
        {plugins.length === 0 ? (
          <p className="muted">
            No plugins installed. Add a plugin folder to get started.
          </p>
        ) : (
          <ul className="extensions-list">
            {plugins.map((plugin) => (
              <PluginCard
                key={plugin.manifest.id}
                plugin={plugin}
                busy={busy === plugin.manifest.id}
                expanded={expanded === plugin.manifest.id}
                history={history[plugin.manifest.id] ?? null}
                historyLoading={historyLoading === plugin.manifest.id}
                clearHistoryBusy={busy === plugin.manifest.id + ":history"}
                onToggle={() => void toggleEnabled(plugin)}
                onRemove={() => void removePlugin(plugin)}
                onToggleHistory={() => void toggleHistory(plugin)}
                onClearHistory={() => void clearHistory(plugin)}
              />
            ))}
          </ul>
        )}
        <div className="settings-inline-actions" style={{ marginTop: "12px" }}>
          <button
            type="button"
            className="primary"
            disabled={busy !== null}
            onClick={() => void addPlugin()}
          >
            + Add plugin folder…
          </button>
          <button type="button" disabled={loading} onClick={() => void refresh()}>
            Refresh
          </button>
        </div>
      </SettingsBlock>

      <SettingsBlock title="About extensions">
        <p className="muted settings-note">
          Extensions are small local JavaScript plugins that run actions on selected photos.
          They never access the network. Each plugin declares its permissions in its manifest
          and the host enforces them before every call.
        </p>
        <p className="muted settings-note">
          To install: drop a plugin folder (containing <code>lumora.plugin.json</code>)
          and click "Add plugin folder…". To remove: click Remove next to the plugin.
        </p>
      </SettingsBlock>
    </>
  );
}

function PluginCard({
  plugin,
  busy,
  expanded,
  history,
  historyLoading,
  clearHistoryBusy,
  onToggle,
  onRemove,
  onToggleHistory,
  onClearHistory,
}: {
  plugin: PluginEntry;
  busy: boolean;
  expanded: boolean;
  history: PluginRunRecord[] | null;
  historyLoading: boolean;
  clearHistoryBusy: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onToggleHistory: () => void;
  onClearHistory: () => void;
}) {
  const m = plugin.manifest;
  const lastRun = history?.[0];

  return (
    <li className="extension-card">
      <div className="extension-card-header">
        <div className="extension-card-meta">
          <strong className="extension-card-name">{m.name}</strong>
          <span className="muted extension-card-id">{m.id} · v{m.version}</span>
          {m.description && (
            <span className="muted extension-card-desc">{m.description}</span>
          )}
        </div>
        <div className="extension-card-controls">
          <label className="toggle-label" title={plugin.enabled ? "Disable" : "Enable"}>
            <input
              type="checkbox"
              checked={plugin.enabled}
              disabled={busy}
              onChange={onToggle}
              aria-label={`${plugin.enabled ? "Disable" : "Enable"} ${m.name}`}
            />
            <span className="toggle-track" aria-hidden="true" />
          </label>
        </div>
      </div>

      {m.permissions.length > 0 && (
        <div className="extension-permissions">
          <span className="muted" style={{ fontSize: "11px", marginRight: "6px" }}>
            Permissions:
          </span>
          {m.permissions.map((perm) => (
            <span key={perm} className="extension-permission-badge">
              {PERMISSION_LABELS[perm] ?? perm}
            </span>
          ))}
        </div>
      )}

      <div className="extension-card-footer">
        {lastRun ? (
          <span className="muted" style={{ fontSize: "11px" }}>
            Last run: {new Date(lastRun.startedAt).toLocaleDateString()} —{" "}
            <span
              style={{
                color:
                  lastRun.outcome === "ok"
                    ? "var(--color-success, #27ae60)"
                    : lastRun.outcome === "error"
                    ? "var(--color-error, #c0392b)"
                    : undefined,
              }}
            >
              {OUTCOME_BADGE[lastRun.outcome] ?? lastRun.outcome}
            </span>
          </span>
        ) : (
          <span className="muted" style={{ fontSize: "11px" }}>
            Never run
          </span>
        )}

        <div className="extension-card-actions">
          <button type="button" onClick={onToggleHistory} style={{ fontSize: "12px" }}>
            {expanded ? "Hide history" : "History"}
          </button>
          <button
            type="button"
            disabled={busy}
            style={{ fontSize: "12px", color: "var(--color-error, #c0392b)" }}
            onClick={onRemove}
          >
            Remove
          </button>
        </div>
      </div>

      {expanded && (
        <div className="extension-history">
          {historyLoading ? (
            <p className="muted" style={{ fontSize: "12px" }}>Loading history…</p>
          ) : !history || history.length === 0 ? (
            <p className="muted" style={{ fontSize: "12px" }}>No runs recorded yet.</p>
          ) : (
            <ul className="extension-history-list">
              {history.map((rec) => (
                <HistoryRow key={rec.runId} record={rec} />
              ))}
            </ul>
          )}
          {history && history.length > 0 && (
            <button
              type="button"
              disabled={clearHistoryBusy}
              style={{ fontSize: "11px", marginTop: "6px" }}
              onClick={onClearHistory}
            >
              Clear history
            </button>
          )}
        </div>
      )}
    </li>
  );
}

function HistoryRow({ record }: { record: PluginRunRecord }) {
  const [open, setOpen] = useState(false);
  return (
    <li className="extension-history-row">
      <button
        type="button"
        className="extension-history-summary"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span
          className="extension-history-outcome"
          style={{
            color:
              record.outcome === "ok"
                ? "var(--color-success, #27ae60)"
                : record.outcome === "error"
                ? "var(--color-error, #c0392b)"
                : undefined,
          }}
        >
          {OUTCOME_BADGE[record.outcome] ?? record.outcome}
        </span>
        <span className="muted">
          {new Date(record.startedAt).toLocaleString()} · {record.durationMs}ms ·{" "}
          {record.assetsAffected}/{record.assetsRequested} assets
        </span>
        <span style={{ marginLeft: "auto" }}>{open ? "▲" : "▼"}</span>
      </button>
      {open && record.logLines.length > 0 && (
        <ul className="extension-history-logs">
          {record.logLines.map((line, i) => (
            <li key={i} className={`log-level-${line.level}`}>
              <span className="log-ts">+{line.timestampMs}ms</span>
              <span>{line.message}</span>
            </li>
          ))}
        </ul>
      )}
      {open && record.errorMessage && (
        <p className="muted" style={{ color: "var(--color-error, #c0392b)", fontSize: "11px" }}>
          {record.errorCode}: {record.errorMessage}
        </p>
      )}
    </li>
  );
}
