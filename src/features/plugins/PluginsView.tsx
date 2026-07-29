import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { PageHeader } from "../../components/PageHeader";
import { MAKER } from "../../lib/maker";
import {
  PluginEditorPanel,
  type PluginEditorInitial,
} from "./PluginEditorPanel";
import { api, type AvailablePlugin, type PluginEntry, type PluginManifest, type PluginRunRecord } from "../../lib/tauri";

// ─── Constants ───────────────────────────────────────────────────────────────

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

const PERM_ICONS: Record<string, string> = {
  "read:assets": "👁",
  "read:metadata": "🏷",
  "write:metadata": "✏️",
  "rename:filesystem": "✍️",
  "move:filesystem": "📂",
  "copy:filesystem": "📋",
  "delete:filesystem": "🗑",
  "export:assets": "📤",
};

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

// ─── Plugin category chips (derived from id prefix / permissions) ─────────────
function guessCategory(id: string, perms: string[]): string {
  if (id.includes("rename") || perms.includes("rename:filesystem")) return "Rename";
  if (id.includes("export") || perms.includes("export:assets")) return "Export";
  if (id.includes("organiz") || perms.includes("move:filesystem")) return "Organize";
  if (id.includes("hello") || perms.length === 0) return "Demo";
  return "Utility";
}

// ─── Root view ────────────────────────────────────────────────────────────────

type Tab = "installed" | "discover";

export function PluginsView() {
  const [tab, setTab] = useState<Tab>("discover");
  const [editorSession, setEditorSession] = useState<PluginEditorInitial | null>(null);
  const [plugins, setPlugins] = useState<PluginEntry[]>([]);
  const [available, setAvailable] = useState<AvailablePlugin[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [installed, avail] = await Promise.all([
        api.listPlugins(),
        api.listAvailablePlugins(),
      ]);
      setPlugins(installed);
      setAvailable(avail);
      setError(null);
      // Auto-switch to installed tab once they have something there
      if (installed.length > 0 && tab === "discover") {
        // Don't forcibly redirect; let the user decide.
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const handleInstallFromSource = async (sourceDir: string, id: string) => {
    setBusy(id);
    try {
      const ids = await api.installPluginDir(sourceDir);
      await Promise.all(ids.map((installedId) => api.setPluginEnabled(installedId, true)));
      await refresh();
      setTab("installed");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const handleAddFolder = async () => {
    const dir = await open({
      directory: true,
      multiple: false,
      title: "Import a plugin folder",
    });
    if (!dir || typeof dir !== "string") return;
    setBusy("__folder__");
    try {
      const ids = await api.installPluginDir(dir);
      await Promise.all(ids.map((id) => api.setPluginEnabled(id, true)));
      await refresh();
      setTab("installed");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const handleToggle = async (plugin: PluginEntry) => {
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

  const handleRemove = async (plugin: PluginEntry) => {
    if (
      !window.confirm(
        `Remove "${plugin.manifest.name}"?\n\nThe plugin folder and its run history will be permanently deleted.`,
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

  const openEditor = (initial: PluginEditorInitial) => {
    setEditorSession(initial);
  };

  const closeEditor = () => {
    setEditorSession(null);
  };

  const handleSaved = () => {
    void refresh();
    closeEditor();
    setTab("installed");
  };

  const openPluginDocs = () => {
    void openUrl(MAKER.pluginGuideUrl);
  };

  if (editorSession) {
    return (
      <div className="plugins-page plugins-page--editor">
        {error && (
          <div className="plugins-error" role="alert">
            <span>{error}</span>
            <button type="button" onClick={() => setError(null)} aria-label="Dismiss">
              ✕
            </button>
          </div>
        )}
        <PluginEditorPanel
          initial={editorSession}
          busy={busy === "__create__"}
          onBusyChange={(v) => setBusy(v ? "__create__" : null)}
          onSaved={handleSaved}
          onError={setError}
          onCancel={closeEditor}
          onOpenDocs={openPluginDocs}
        />
      </div>
    );
  }

  return (
    <div className="plugins-page">
      <PageHeader
        title="Plugins"
        description="Discover, import, and create JavaScript actions for your photo selection."
      />

      {error && (
        <div className="plugins-error" role="alert">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)} aria-label="Dismiss">
            ✕
          </button>
        </div>
      )}

      {/* Tab bar */}
      <div className="plugins-tabbar">
        <button
          type="button"
          className={`plugins-tab ${tab === "discover" ? "is-active" : ""}`}
          onClick={() => setTab("discover")}
        >
          Discover
          {available.filter((p) => !p.installed).length > 0 && (
            <span className="plugins-tab-badge">
              {available.filter((p) => !p.installed).length}
            </span>
          )}
        </button>
        <button
          type="button"
          className={`plugins-tab ${tab === "installed" ? "is-active" : ""}`}
          onClick={() => setTab("installed")}
        >
          Installed
          {plugins.length > 0 && (
            <span className="plugins-tab-badge plugins-tab-badge--count">
              {plugins.length}
            </span>
          )}
        </button>

        <div className="plugins-tabbar-spacer" />

        <button
          type="button"
          className="plugins-toolbar-btn"
          onClick={openPluginDocs}
        >
          Documentation
        </button>

        <button
          type="button"
          className="plugins-toolbar-btn"
          disabled={busy !== null}
          onClick={() => void handleAddFolder()}
        >
          Import Plugin
        </button>

        <button
          type="button"
          className="plugins-toolbar-btn primary"
          disabled={busy !== null}
          onClick={() => openEditor({ mode: "create" })}
        >
          Create new plugin
        </button>

        <button
          type="button"
          className="plugins-icon-btn"
          title="Refresh"
          disabled={loading}
          onClick={() => void refresh()}
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor" aria-hidden="true">
            <path d="M17.65 6.35A7.96 7.96 0 0 0 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0 1 12 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z" />
          </svg>
        </button>
      </div>

      {loading ? (
        <div className="plugins-loading" role="status">
          <span className="spinner" aria-hidden="true" />
          Loading…
        </div>
      ) : tab === "discover" ? (
        <DiscoverTab
          available={available}
          busy={busy}
          onInstall={handleInstallFromSource}
          onCustomize={(plugin) =>
            openEditor({
              mode: "fork",
              sourceDir: plugin.sourceDir,
              sourcePluginId: plugin.installed ? plugin.manifest.id : undefined,
            })
          }
        />
      ) : (
        <InstalledTab
          plugins={plugins}
          busy={busy}
          onToggle={handleToggle}
          onRemove={handleRemove}
          onAddFolder={() => void handleAddFolder()}
          onEdit={(plugin) =>
            openEditor({ mode: "edit", pluginId: plugin.manifest.id })
          }
          onFork={(plugin) =>
            openEditor({
              mode: "fork",
              sourcePluginId: plugin.manifest.id,
            })
          }
        />
      )}
    </div>
  );
}

// ─── Discover tab ─────────────────────────────────────────────────────────────

function DiscoverTab({
  available,
  busy,
  onInstall,
  onCustomize,
}: {
  available: AvailablePlugin[];
  busy: string | null;
  onInstall: (sourceDir: string, id: string) => void;
  onCustomize: (plugin: AvailablePlugin) => void;
}) {
  if (available.length === 0) {
    return (
      <div className="plugins-discover-empty">
        <p className="muted">No first-party plugins found.</p>
        <p className="muted" style={{ fontSize: "12px" }}>
          Make sure you have cloned the repository — example plugins live in{" "}
          <code>plugins/examples/</code>.
        </p>
      </div>
    );
  }

  const notInstalled = available.filter((p) => !p.installed);
  const installed = available.filter((p) => p.installed);

  return (
    <div className="plugins-discover">
      {notInstalled.length > 0 && (
        <section className="plugins-discover-section">
          <h3 className="plugins-discover-heading">
            First-party plugins
            <span className="plugins-discover-count">{notInstalled.length}</span>
          </h3>
          <div className="plugins-card-grid">
            {notInstalled.map((p) => (
              <AvailableCard
                key={p.manifest.id}
                plugin={p}
                busy={busy === p.manifest.id}
                onInstall={() => onInstall(p.sourceDir, p.manifest.id)}
                onCustomize={() => onCustomize(p)}
              />
            ))}
          </div>
        </section>
      )}

      {installed.length > 0 && (
        <section className="plugins-discover-section">
          <h3 className="plugins-discover-heading plugins-discover-heading--muted">
            Already installed
          </h3>
          <div className="plugins-card-grid">
            {installed.map((p) => (
              <AvailableCard
                key={p.manifest.id}
                plugin={p}
                busy={false}
                onInstall={() => {}}
                onCustomize={() => onCustomize(p)}
              />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function AvailableCard({
  plugin,
  busy,
  onInstall,
  onCustomize,
}: {
  plugin: AvailablePlugin;
  busy: boolean;
  onInstall: () => void;
  onCustomize: () => void;
}) {
  const m = plugin.manifest;
  const category = guessCategory(m.id, m.permissions);

  return (
    <div className={`plugin-avail-card ${plugin.installed ? "is-installed" : ""}`}>
      <div className="plugin-avail-header">
        <div className="plugin-avail-icon" aria-hidden="true">
          {categoryIcon(category)}
        </div>
        <div className="plugin-avail-meta">
          <span className="plugin-avail-category">{category}</span>
          <strong className="plugin-avail-name">{m.name}</strong>
        </div>
        {plugin.installed && (
          <span className="plugin-avail-installed-badge">
            <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor"><path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41L9 16.17z" /></svg>
            Installed
          </span>
        )}
      </div>

      {m.description && (
        <p className="plugin-avail-desc">{m.description}</p>
      )}

      {m.permissions.length > 0 ? (
        <div className="plugin-avail-perms">
          {m.permissions.map((perm) => (
            <span key={perm} className="plugin-perm-pill" title={PERMISSION_LABELS[perm] ?? perm}>
              {PERM_ICONS[perm] ?? "🔑"} {PERMISSION_LABELS[perm] ?? perm}
            </span>
          ))}
          <span className="plugin-perm-pill plugin-perm-pill--blocked" title="No network access">
            🚫 Network
          </span>
        </div>
      ) : (
        <div className="plugin-avail-perms">
          <span className="plugin-perm-pill plugin-perm-pill--safe">No permissions needed</span>
        </div>
      )}

      <div className="plugin-avail-footer">
        <span className="plugin-avail-version muted">
          v{m.version}
          {m.author ? ` · ${m.author}` : ""}
        </span>
        <div className="plugin-avail-footer-actions">
          <button
            type="button"
            className="plugin-ghost-btn"
            disabled={busy}
            onClick={onCustomize}
          >
            Customize
          </button>
          {!plugin.installed && (
            <button
              type="button"
              className="plugin-install-btn"
              disabled={busy}
              onClick={onInstall}
            >
              {busy ? (
                <>
                  <span className="spinner-xs" aria-hidden="true" />
                  Installing…
                </>
              ) : (
                <>
                  <svg viewBox="0 0 24 24" width="13" height="13" fill="currentColor" aria-hidden="true">
                    <path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z" />
                  </svg>
                  Install
                </>
              )}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function categoryIcon(category: string) {
  switch (category) {
    case "Rename":
      return (
        <svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor">
          <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 0 0 0-1.41l-2.34-2.34a1 1 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" />
        </svg>
      );
    case "Export":
      return (
        <svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor">
          <path d="M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z" />
        </svg>
      );
    case "Organize":
      return (
        <svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor">
          <path d="M20 6h-8l-2-2H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2z" />
        </svg>
      );
    default:
      return (
        <svg viewBox="0 0 24 24" width="22" height="22" fill="currentColor">
          <path d="M20.5 11H19V7a2 2 0 0 0-2-2h-4V3.5a2.5 2.5 0 0 0-5 0V5H4a2 2 0 0 0-2 2v3.8h1.5c1.5 0 2.7 1.2 2.7 2.7S5 16.2 3.5 16.2H2V20a2 2 0 0 0 2 2h3.8v-1.5a2.7 2.7 0 0 1 5.4 0V22H17a2 2 0 0 0 2-2v-4h1.5a2.5 2.5 0 0 0 0-5z" />
        </svg>
      );
  }
}

// ─── Installed tab ────────────────────────────────────────────────────────────

function InstalledTab({
  plugins,
  busy,
  onToggle,
  onRemove,
  onAddFolder,
  onEdit,
  onFork,
}: {
  plugins: PluginEntry[];
  busy: string | null;
  onToggle: (p: PluginEntry) => void;
  onRemove: (p: PluginEntry) => void;
  onAddFolder: () => void;
  onEdit: (p: PluginEntry) => void;
  onFork: (p: PluginEntry) => void;
}) {
  if (plugins.length === 0) {
    return (
      <div className="plugins-installed-empty">
        <div className="plugins-empty-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="48" height="48" fill="currentColor">
            <path d="M20.5 11H19V7a2 2 0 0 0-2-2h-4V3.5a2.5 2.5 0 0 0-5 0V5H4a2 2 0 0 0-2 2v3.8h1.5c1.5 0 2.7 1.2 2.7 2.7S5 16.2 3.5 16.2H2V20a2 2 0 0 0 2 2h3.8v-1.5a2.7 2.7 0 0 1 5.4 0V22H17a2 2 0 0 0 2-2v-4h1.5a2.5 2.5 0 0 0 0-5z" />
          </svg>
        </div>
        <h3>No plugins installed yet</h3>
        <p className="muted">
          Head to <strong>Discover</strong> to install first-party plugins in one click,
          or add your own from a local folder.
        </p>
        <button type="button" className="primary" onClick={onAddFolder}>
          Import Plugin
        </button>
      </div>
    );
  }

  return (
    <ul className="plugins-list">
      {plugins.map((plugin) => (
        <PluginCard
          key={plugin.manifest.id}
          plugin={plugin}
          busy={busy === plugin.manifest.id}
          onToggle={() => onToggle(plugin)}
          onRemove={() => onRemove(plugin)}
          onEdit={() => onEdit(plugin)}
          onFork={() => onFork(plugin)}
        />
      ))}
    </ul>
  );
}

// ─── Installed plugin card ────────────────────────────────────────────────────

function PluginCard({
  plugin,
  busy,
  onToggle,
  onRemove,
  onEdit,
  onFork,
}: {
  plugin: PluginEntry;
  busy: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onEdit: () => void;
  onFork: () => void;
}) {
  const m = plugin.manifest;
  const [expanded, setExpanded] = useState(false);
  const [history, setHistory] = useState<PluginRunRecord[] | null>(null);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [clearBusy, setClearBusy] = useState(false);
  const hasFetched = useRef(false);

  const loadHistory = useCallback(async (force = false) => {
    if (hasFetched.current && !force) return;
    hasFetched.current = true;
    setHistoryLoading(true);
    try {
      setHistory(await api.getPluginHistory(m.id, 20));
    } catch {
      setHistory([]);
    } finally {
      setHistoryLoading(false);
    }
  }, [m.id]);

  useEffect(() => {
    hasFetched.current = false;
    setHistory(null);
    void loadHistory(true);
  }, [loadHistory]);

  const handleToggleHistory = () => {
    const next = !expanded;
    setExpanded(next);
    if (next && history === null) void loadHistory(true);
  };

  const handleClearHistory = async () => {
    setClearBusy(true);
    try {
      await api.clearPluginHistory(m.id);
      setHistory([]);
    } catch {
      /* ignore */
    } finally {
      setClearBusy(false);
    }
  };

  const lastRun = history?.[0];

  return (
    <li className={`plugin-card ${plugin.enabled ? "is-enabled" : "is-disabled"}`}>
      <div className="plugin-card-main">
        <div className="plugin-card-identity">
          <div className="plugin-card-title-row">
            <strong className="plugin-card-name">{m.name}</strong>
            <span className={`plugin-status-pill ${plugin.enabled ? "enabled" : "disabled"}`}>
              {plugin.enabled ? "Enabled" : "Disabled"}
            </span>
          </div>
          <span className="muted plugin-card-id">
            {m.id} · v{m.version}
            {m.author ? ` · by ${m.author}` : ""}
          </span>
          {m.description && (
            <p className="plugin-card-desc muted">{m.description}</p>
          )}
        </div>

        <div className="plugin-card-toggle">
          <button
            type="button"
            className={`plugin-toggle-btn ${plugin.enabled ? "active" : ""}`}
            disabled={busy}
            onClick={onToggle}
            aria-pressed={plugin.enabled}
            aria-label={`${plugin.enabled ? "Disable" : "Enable"} ${m.name}`}
          >
            {plugin.enabled ? "Disable" : "Enable"}
          </button>
        </div>
      </div>

      {m.permissions.length > 0 && (
        <div className="plugin-card-permissions">
          {m.permissions.map((perm) => (
            <span key={perm} className="plugin-permission-chip">
              {PERM_ICONS[perm] ?? "🔑"} {PERMISSION_LABELS[perm] ?? perm}
            </span>
          ))}
          <span className="plugin-permission-chip blocked">🚫 No Network</span>
        </div>
      )}

      {m.contributions.actions.length > 0 && (
        <div className="plugin-card-actions-row">
          <span className="muted" style={{ fontSize: "11px" }}>Actions: </span>
          {m.contributions.actions.map((action) => (
            <span key={action.id} className="plugin-action-chip">
              {action.label}
            </span>
          ))}
        </div>
      )}

      <PluginUsageInstructions manifest={m} />

      <div className="plugin-card-footer">
        <span className="plugin-last-run">
          {historyLoading && history === null ? (
            <span className="muted">Loading last run…</span>
          ) : lastRun ? (
            <>
              Last run {new Date(lastRun.startedAt).toLocaleDateString()} —{" "}
              <span style={{ color: OUTCOME_COLORS[lastRun.outcome] }}>
                {OUTCOME_LABEL[lastRun.outcome] ?? lastRun.outcome}
              </span>
              {" · "}
              {lastRun.assetsAffected}/{lastRun.assetsRequested} assets
            </>
          ) : (
            <span className="muted">Never run</span>
          )}
        </span>

        <div className="plugin-footer-btns">
          <button type="button" className="plugin-ghost-btn" onClick={onEdit}>
            Edit
          </button>
          <button type="button" className="plugin-ghost-btn" onClick={onFork}>
            Save copy
          </button>
          <button type="button" className="plugin-ghost-btn" onClick={handleToggleHistory}>
            {expanded ? "Hide history" : "History"}
          </button>
          <button
            type="button"
            className="plugin-ghost-btn danger"
            disabled={busy}
            onClick={onRemove}
          >
            Remove
          </button>
        </div>
      </div>

      {expanded && (
        <div className="plugin-history">
          {historyLoading ? (
            <p className="muted plugin-history-empty">Loading history…</p>
          ) : !history || history.length === 0 ? (
            <p className="muted plugin-history-empty">No runs recorded yet.</p>
          ) : (
            <ul className="plugin-history-list">
              {history.map((rec) => (
                <HistoryRow key={rec.runId} record={rec} />
              ))}
            </ul>
          )}
          {history && history.length > 0 && (
            <button
              type="button"
              className="plugin-ghost-btn"
              disabled={clearBusy}
              style={{ marginTop: "8px", fontSize: "11px" }}
              onClick={() => void handleClearHistory()}
            >
              Clear history
            </button>
          )}
        </div>
      )}
    </li>
  );
}

// ─── Usage instruction helpers ───────────────────────────────────────────────

function buildUsageSteps(m: PluginManifest): string[] {
  const actions = m.contributions.actions;
  if (actions.length === 0) return [];
  const steps: string[] = [];
  steps.push("Select one or more photos in your library.");

  const actionNames = actions.map((a) => `"${a.label}"`).join(" or ");
  steps.push(`Click Plugins in the selection bar and choose ${actionNames}.`);

  const firstAction = actions[0];
  if (firstAction.minSelection && firstAction.minSelection > 1) {
    steps.push(`Requires at least ${firstAction.minSelection} photo${firstAction.minSelection > 1 ? "s" : ""} selected.`);
  }
  if (firstAction.maxSelection) {
    steps.push(`Works on up to ${firstAction.maxSelection} photos at a time.`);
  }
  if (m.permissions.includes("rename:filesystem")) {
    steps.push("Files will be renamed in place — originals are not duplicated.");
  }
  if (m.permissions.includes("export:assets")) {
    steps.push("A save-dialog will appear to choose the export destination.");
  }
  if (m.permissions.includes("move:filesystem")) {
    steps.push("Photos will be moved into the new folder structure on disk.");
  }
  return steps;
}

/** Collapsible usage block shown on installed cards */
function PluginUsageInstructions({ manifest }: { manifest: PluginManifest }) {
  const [open, setOpen] = useState(false);
  const steps = buildUsageSteps(manifest);
  if (steps.length === 0) return null;

  return (
    <div className="plugin-usage">
      <button
        type="button"
        className="plugin-usage-toggle"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <svg viewBox="0 0 24 24" width="13" height="13" fill="currentColor" aria-hidden="true">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z" />
        </svg>
        How to use
        <span className="plugin-usage-chevron" aria-hidden="true">{open ? "▲" : "▼"}</span>
      </button>

      {open && (
        <ol className="plugin-usage-steps">
          {steps.map((step, i) => (
            <li key={i}>{step}</li>
          ))}
        </ol>
      )}
    </div>
  );
}


function HistoryRow({ record }: { record: PluginRunRecord }) {
  const [open, setOpen] = useState(false);
  return (
    <li className="plugin-history-row">
      <button
        type="button"
        className="plugin-history-summary"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="plugin-history-outcome" style={{ color: OUTCOME_COLORS[record.outcome] }}>
          {OUTCOME_LABEL[record.outcome] ?? record.outcome}
        </span>
        <span className="muted plugin-history-meta">
          {new Date(record.startedAt).toLocaleString()} · {record.durationMs}ms ·{" "}
          {record.mode} · {record.assetsAffected}/{record.assetsRequested} assets
        </span>
        <span className="plugin-history-chevron" aria-hidden="true">
          {open ? "▲" : "▼"}
        </span>
      </button>

      {open && (
        <div className="plugin-history-detail">
          {record.errorMessage && (
            <p className="plugin-history-error">
              <strong>{record.errorCode}</strong>: {record.errorMessage}
            </p>
          )}
          {record.logLines.length > 0 ? (
            <ul className="plugin-log-lines">
              {record.logLines.map((line, i) => (
                <li key={i} className={`plugin-log-line log-${line.level}`}>
                  <span className="log-ts">+{line.timestampMs}ms</span>
                  <span className={`log-level log-level-${line.level}`}>{line.level}</span>
                  <span className="log-msg">{line.message}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted" style={{ fontSize: "11px" }}>No log lines.</p>
          )}
        </div>
      )}
    </li>
  );
}
