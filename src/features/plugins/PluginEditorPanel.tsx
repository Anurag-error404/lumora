import { useEffect, useMemo, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { api } from "../../lib/tauri";
import {
  analyzeDraft,
  slugify,
  suggestForkId,
} from "./pluginAnalysis";
import {
  PERMISSION_HINTS,
  PERMISSION_LABELS,
  STARTER_MAIN_JS,
} from "./pluginGuideContent";
import { PluginCodeEditor } from "./PluginCodeEditor";

export type PluginEditorMode = "create" | "edit" | "fork";

export type PluginEditorInitial = {
  mode: PluginEditorMode;
  pluginId?: string;
  sourcePluginId?: string;
  sourceDir?: string;
  name?: string;
  description?: string;
  author?: string;
  actionId?: string;
  actionLabel?: string;
  mainJs?: string;
};

export function PluginEditorPanel({
  initial,
  busy,
  onBusyChange,
  onSaved,
  onError,
  onCancel,
  onOpenDocs,
}: {
  initial?: PluginEditorInitial | null;
  busy: boolean;
  onBusyChange: (busy: boolean) => void;
  onSaved: () => void;
  onError: (message: string | null) => void;
  onCancel?: () => void;
  onOpenDocs?: () => void;
}) {
  const mode = initial?.mode ?? "create";

  const [name, setName] = useState(initial?.name ?? "");
  const [pluginId, setPluginId] = useState(
    initial?.pluginId ?? (mode === "fork" && initial?.sourcePluginId
      ? suggestForkId(initial.sourcePluginId)
      : "com.personal.my-plugin"),
  );
  const [description, setDescription] = useState(initial?.description ?? "");
  const [author, setAuthor] = useState(initial?.author ?? "");
  const [actionLabel, setActionLabel] = useState(initial?.actionLabel ?? "");
  const [actionId, setActionId] = useState(initial?.actionId ?? "my-action");
  const [mainJs, setMainJs] = useState(initial?.mainJs ?? STARTER_MAIN_JS);
  const [successDir, setSuccessDir] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [idTouched, setIdTouched] = useState(mode !== "create");
  const [actionIdTouched, setActionIdTouched] = useState(mode !== "create");

  useEffect(() => {
    if (!initial) return;
    let cancelled = false;

    const load = async () => {
      setLoading(true);
      try {
        if (initial.mode === "edit" && initial.pluginId) {
          const sources = await api.readPluginSources(initial.pluginId);
          if (cancelled) return;
          const action = sources.manifest.contributions.actions[0];
          setName(sources.manifest.name);
          setPluginId(sources.manifest.id);
          setDescription(sources.manifest.description);
          setAuthor(sources.manifest.author);
          setActionId(action?.id ?? "my-action");
          setActionLabel(action?.label ?? "");
          setMainJs(sources.mainJs);
        } else if (initial.mode === "fork") {
          const sources = initial.sourceDir
            ? await api.readPluginSourcesFromDir(initial.sourceDir)
            : initial.sourcePluginId
              ? await api.readPluginSources(initial.sourcePluginId)
              : null;
          if (cancelled || !sources) return;
          const action = sources.manifest.contributions.actions[0];
          setName(initial.name ?? `${sources.manifest.name} (personal)`);
          setPluginId(initial.pluginId ?? suggestForkId(sources.manifest.id));
          setDescription(
            initial.description ??
              `Personal fork of ${sources.manifest.name}`,
          );
          setAuthor(initial.author ?? sources.manifest.author);
          setActionId(initial.actionId ?? action?.id ?? "my-action");
          setActionLabel(initial.actionLabel ?? action?.label ?? "");
          setMainJs(initial.mainJs ?? sources.mainJs);
        } else if (initial.mainJs) {
          setMainJs(initial.mainJs);
        }
      } catch (e) {
        if (!cancelled) onError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, [initial, onError]);

  const analysis = useMemo(
    () => analyzeDraft(mainJs, pluginId, name, actionId, actionLabel),
    [mainJs, pluginId, name, actionId, actionLabel],
  );

  const hasErrors = analysis.issues.length > 0;
  const canSave =
    !busy &&
    !loading &&
    name.trim() &&
    pluginId.trim() &&
    actionId.trim() &&
    actionLabel.trim() &&
    !hasErrors;

  const handleNameChange = (value: string) => {
    setName(value);
    if (mode === "create" && !idTouched) {
      const slug = slugify(value);
      setPluginId(slug ? `com.personal.${slug}` : "com.personal.my-plugin");
    }
    if (mode === "create" && !actionIdTouched) {
      const slug = slugify(value);
      setActionId(slug || "my-action");
    }
    if (mode === "create" && !actionLabel) {
      setActionLabel(value ? `${value}…` : "");
    }
  };

  const handleSave = async () => {
    onError(null);
    setSuccessDir(null);
    onBusyChange(true);
    try {
      if (mode === "edit") {
        const result = await api.savePluginDraft({
          pluginId: pluginId.trim(),
          name: name.trim(),
          description: description.trim() || undefined,
          author: author.trim() || undefined,
          actionId: actionId.trim(),
          actionLabel: actionLabel.trim(),
          mainJs,
        });
        setSuccessDir(result.dir);
      } else if (mode === "fork") {
        const result = await api.forkPlugin({
          sourcePluginId: initial?.sourcePluginId,
          sourceDir: initial?.sourceDir,
          newId: pluginId.trim(),
          newName: name.trim(),
          description: description.trim() || undefined,
          author: author.trim() || undefined,
          actionId: actionId.trim(),
          actionLabel: actionLabel.trim(),
          mainJs,
        });
        setSuccessDir(result.dir);
      } else {
        const result = await api.createPlugin({
          id: pluginId.trim(),
          name: name.trim(),
          description: description.trim() || undefined,
          author: author.trim() || undefined,
          actionId: actionId.trim(),
          actionLabel: actionLabel.trim(),
          mainJs,
        });
        setSuccessDir(result.dir);
      }
      onSaved();
    } catch (e) {
      onError(String(e));
    } finally {
      onBusyChange(false);
    }
  };

  const title =
    mode === "edit"
      ? "Edit plugin"
      : mode === "fork"
        ? "Save personal fork"
        : "Create a new plugin";

  const saveLabel =
    mode === "edit"
      ? "Save changes"
      : mode === "fork"
        ? "Save copy"
        : "Create plugin";

  const modeLabel =
    mode === "edit" ? "Editing" : mode === "fork" ? "Fork" : "New";

  return (
    <div className="plugins-editor-page">
      <header className="plugins-editor-toolbar">
        <div className="plugins-editor-toolbar-left">
          <button type="button" className="plugins-editor-back" onClick={onCancel}>
            ← Back
          </button>
          <div className="plugins-editor-toolbar-title">
            <h2>{title}</h2>
            <span className="plugins-editor-mode-badge">{modeLabel}</span>
          </div>
        </div>
        <div className="plugins-editor-toolbar-actions">
          {onOpenDocs && (
            <button type="button" className="secondary" onClick={onOpenDocs}>
              Documentation
            </button>
          )}
          {onCancel && (
            <button type="button" className="secondary" onClick={onCancel}>
              Cancel
            </button>
          )}
          <button
            type="button"
            className="primary"
            disabled={!canSave}
            onClick={() => void handleSave()}
          >
            {busy ? "Saving…" : saveLabel}
          </button>
        </div>
      </header>

      {loading ? (
        <div className="plugins-loading plugins-editor-loading" role="status">
          <span className="spinner" aria-hidden="true" />
          Loading plugin…
        </div>
      ) : (
        <>
          <section className="plugins-editor-meta-bar">
            <div className="plugins-editor-meta-grid">
              <label className="plugins-field">
                <span>Name</span>
                <input
                  type="text"
                  value={name}
                  placeholder="My batch rename"
                  disabled={mode === "edit"}
                  onChange={(e) => handleNameChange(e.target.value)}
                />
              </label>

              <label className="plugins-field">
                <span>Plugin id</span>
                <input
                  type="text"
                  value={pluginId}
                  placeholder="com.personal.my-plugin"
                  disabled={mode === "edit"}
                  onChange={(e) => {
                    setIdTouched(true);
                    setPluginId(e.target.value);
                  }}
                />
              </label>

              <label className="plugins-field">
                <span>Author</span>
                <input
                  type="text"
                  value={author}
                  placeholder="Your name"
                  onChange={(e) => setAuthor(e.target.value)}
                />
              </label>

              <label className="plugins-field">
                <span>Action label</span>
                <input
                  type="text"
                  value={actionLabel}
                  placeholder="Run my action…"
                  onChange={(e) => setActionLabel(e.target.value)}
                />
              </label>

              <label className="plugins-field">
                <span>Action id</span>
                <input
                  type="text"
                  value={actionId}
                  placeholder="my-action"
                  onChange={(e) => {
                    setActionIdTouched(true);
                    setActionId(e.target.value);
                  }}
                />
              </label>

              <label className="plugins-field plugins-field--wide">
                <span>Description</span>
                <input
                  type="text"
                  value={description}
                  placeholder="What does this plugin do?"
                  onChange={(e) => setDescription(e.target.value)}
                />
              </label>
            </div>
          </section>

          <div className="plugins-editor-workbench">
            <section className="plugins-editor-code-panel">
              <div className="plugins-editor-file-tabs">
                <span className="plugins-editor-file-tab is-active">
                  <span className="plugins-editor-file-icon">JS</span>
                  main.js
                </span>
                <span className="plugins-editor-file-hint">
                  Type <kbd>lumora.</kbd> or <kbd>context.</kbd> for autocomplete
                </span>
              </div>
              <PluginCodeEditor value={mainJs} onChange={setMainJs} />
            </section>

            <aside className="plugins-editor-inspector" aria-live="polite">
              <div className="plugins-inspector-section">
                <h3>Validation</h3>
                {analysis.issues.length === 0 && analysis.warnings.length === 0 ? (
                  <div className="plugins-inspector-status plugins-inspector-status--ok">
                    Structure looks good
                  </div>
                ) : (
                  <ul className="plugins-inspector-list">
                    {analysis.issues.map((issue) => (
                      <li key={issue.code} className="plugins-inspector-item is-error">
                        {issue.message}
                      </li>
                    ))}
                    {analysis.warnings.map((issue) => (
                      <li key={issue.code} className="plugins-inspector-item is-warn">
                        {issue.message}
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              <div className="plugins-inspector-section">
                <h3>Inferred permissions</h3>
                {analysis.permissions.length === 0 ? (
                  <p className="plugins-inspector-muted">None — logging only</p>
                ) : (
                  <ul className="plugins-inferred-perms">
                    {analysis.permissions.map((perm) => (
                      <li key={perm} className="plugin-perm-pill" title={PERMISSION_HINTS[perm]}>
                        {PERMISSION_LABELS[perm] ?? perm}
                      </li>
                    ))}
                  </ul>
                )}
                <p className="plugins-inspector-footnote">
                  Saved to <code>lumora.plugin.json</code> on create
                </p>
              </div>

              {successDir && (
                <div className="plugins-inspector-success" role="status">
                  <strong>{mode === "edit" ? "Saved" : "Created & enabled"}</strong>
                  <code>{successDir}</code>
                  <button type="button" className="secondary" onClick={() => void openPath(successDir)}>
                    Open folder
                  </button>
                </div>
              )}
            </aside>
          </div>
        </>
      )}
    </div>
  );
}
