import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { formatBytes } from "../../lib/format";
import {
  api,
  type LibraryOptionStatus,
  type ModelProgressEvent,
} from "../../lib/tauri";

const CAPABILITY_ORDER = [
  "semanticSearch",
  "ocr",
  "faces",
  "autoTags",
  "captions",
  "memoryProse",
  "duplicates",
  "blurDetection",
];

const IMPORTABLE = new Set(["autoTags", "semanticSearch"]);

/**
 * Pluggable backends per AI capability: install, activate, import local ONNX,
 * and re-run.
 */
export function ModelLibraryPanel() {
  const [options, setOptions] = useState<LibraryOptionStatus[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<ModelProgressEvent | null>(null);
  const [importNote, setImportNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setOptions(await api.modelLibrary());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<ModelProgressEvent>("model-progress", (event) => {
      setDownload(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const groups = useMemo(() => {
    const map = new Map<string, LibraryOptionStatus[]>();
    for (const opt of options) {
      const list = map.get(opt.capability) ?? [];
      list.push(opt);
      map.set(opt.capability, list);
    }
    return CAPABILITY_ORDER.filter((id) => map.has(id)).map((id) => ({
      id,
      label: map.get(id)![0].capabilityLabel,
      options: map.get(id)!,
    }));
  }, [options]);

  const downloadPct =
    download && download.total > 0
      ? Math.min(100, Math.round((100 * download.downloaded) / download.total))
      : null;

  async function activate(opt: LibraryOptionStatus) {
    if (!opt.available || opt.active) return;
    let reprocess = false;
    if (opt.runtime === "onnx") {
      const ok = window.confirm(
        `Switch to ${opt.name}?\n\nDerived data for this capability will be cleared and rebuilt with the new model. Originals are never touched.`,
      );
      if (!ok) return;
      reprocess = true;
    }
    setBusyId(opt.id);
    setError(null);
    setDownload(null);
    try {
      if (!opt.installed && opt.runtime === "onnx" && !opt.id.startsWith("user-")) {
        await api.installModelOption(opt.id);
      }
      const next = await api.setActiveModel(opt.id, reprocess);
      setOptions(next);
    } catch (e) {
      setError(String(e));
      await refresh();
    } finally {
      setBusyId(null);
      setDownload(null);
    }
  }

  async function importLocal(capability: string) {
    setError(null);
    setImportNote(null);
    try {
      if (capability === "autoTags") {
        const modelPath = await open({
          title: "Select AutoTags ONNX model",
          multiple: false,
          filters: [{ name: "ONNX", extensions: ["onnx"] }],
        });
        if (!modelPath || Array.isArray(modelPath)) return;
        const labelsPath = await open({
          title: "Select ImageNet labels .txt (1000 lines)",
          multiple: false,
          filters: [{ name: "Labels", extensions: ["txt"] }],
        });
        if (!labelsPath || Array.isArray(labelsPath)) return;

        setBusyId(`import:${capability}`);
        const report = await api.evaluateLocalAutotags(modelPath, labelsPath);
        if (!report.compatible) {
          setError(`Not compatible: ${report.reasons.join("; ")}`);
          return;
        }
        const ok = window.confirm(
          `Compatible AutoTags model.\n\n${report.reasons.join("\n")}\n\nImport and activate? Derived tags will be rebuilt.`,
        );
        if (!ok) return;
        await api.importLocalAutotags(modelPath, labelsPath, null, true);
        setImportNote(`Imported and activated local AutoTags model.`);
        await refresh();
      } else if (capability === "semanticSearch") {
        const visionPath = await open({
          title: "Select CLIP vision ONNX",
          multiple: false,
          filters: [{ name: "ONNX", extensions: ["onnx"] }],
        });
        if (!visionPath || Array.isArray(visionPath)) return;
        const textPath = await open({
          title: "Select CLIP text ONNX",
          multiple: false,
          filters: [{ name: "ONNX", extensions: ["onnx"] }],
        });
        if (!textPath || Array.isArray(textPath)) return;
        const tokenizerPath = await open({
          title: "Select CLIP tokenizer.json",
          multiple: false,
          filters: [{ name: "Tokenizer", extensions: ["json"] }],
        });
        if (!tokenizerPath || Array.isArray(tokenizerPath)) return;

        setBusyId(`import:${capability}`);
        const report = await api.evaluateLocalClip(
          visionPath,
          textPath,
          tokenizerPath,
        );
        if (!report.compatible) {
          setError(`Not compatible: ${report.reasons.join("; ")}`);
          return;
        }
        const ok = window.confirm(
          `Compatible CLIP bundle.\n\n${report.reasons.join("\n")}\n\nImport and activate? All embeddings will be cleared and rebuilt.`,
        );
        if (!ok) return;
        await api.importLocalClip(
          visionPath,
          textPath,
          tokenizerPath,
          null,
          true,
        );
        setImportNote(`Imported and activated local CLIP model.`);
        await refresh();
      } else {
        setError("Custom import is not supported for this capability yet.");
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="model-library">
      <header className="settings-block-head">
        <div>
          <h3>Model library</h3>
          <p className="muted">
            Pick which backend powers each capability. Downloads only start when
            you ask. You can also import a local ONNX model for Auto-tags or
            Semantic search after a compatibility check. Hugging Face browse is
            not included yet.
          </p>
        </div>
      </header>

      {error && <p className="error-banner">{error}</p>}
      {importNote && <p className="muted">{importNote}</p>}
      {busyId && downloadPct != null && (
        <p className="muted">Downloading… {downloadPct}%</p>
      )}

      {groups.map((group) => (
        <section key={group.id} className="model-library-group">
          <div className="model-library-group-head">
            <h4>{group.label}</h4>
            {IMPORTABLE.has(group.id) && (
              <button
                type="button"
                disabled={busyId != null}
                onClick={() => void importLocal(group.id)}
              >
                {busyId === `import:${group.id}`
                  ? "Evaluating…"
                  : "Import local model…"}
              </button>
            )}
          </div>
          <div className="model-library-options">
            {group.options.map((opt) => (
              <article
                key={opt.id}
                className={`developer-card model-option ${
                  opt.active ? "is-active" : ""
                }`}
              >
                <span className="developer-card-label">
                  {opt.id.startsWith("user-")
                    ? "Local ONNX"
                    : opt.runtime === "onnx"
                      ? "ONNX Runtime"
                      : "Native"}
                </span>
                <strong>{opt.name}</strong>
                <p className="muted">{opt.summary}</p>
                <dl>
                  <div>
                    <dt>License</dt>
                    <dd>{opt.license}</dd>
                  </div>
                  {opt.downloadBytes > 0 && (
                    <div>
                      <dt>Size</dt>
                      <dd>{formatBytes(opt.downloadBytes)}</dd>
                    </div>
                  )}
                  <div>
                    <dt>Status</dt>
                    <dd>
                      {!opt.available
                        ? "Not available yet"
                        : opt.active
                          ? "Active"
                          : opt.installed
                            ? "Installed"
                            : opt.runtime === "native"
                              ? "Built-in"
                              : "Not installed"}
                    </dd>
                  </div>
                </dl>
                {opt.available && !opt.active && (
                  <button
                    type="button"
                    className="primary"
                    disabled={busyId != null}
                    onClick={() => void activate(opt)}
                  >
                    {busyId === opt.id
                      ? "Working…"
                      : opt.installed || opt.runtime === "native"
                        ? "Use this model"
                        : "Download & use"}
                  </button>
                )}
                {opt.active && (
                  <span className="model-option-active-badge">In use</span>
                )}
              </article>
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
