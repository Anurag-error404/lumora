import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
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

/**
 * Pluggable backends per AI capability: install, activate, and re-run.
 */
export function ModelLibraryPanel() {
  const [options, setOptions] = useState<LibraryOptionStatus[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [download, setDownload] = useState<ModelProgressEvent | null>(null);

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
      if (!opt.installed && opt.runtime === "onnx") {
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

  return (
    <div className="model-library">
      <header className="settings-block-head">
        <div>
          <h3>Model library</h3>
          <p className="muted">
            Pick which backend powers each capability. Downloads only start when
            you ask. Switching an ONNX model clears and re-runs that capability.
          </p>
        </div>
      </header>

      {error && <p className="error-banner">{error}</p>}
      {busyId && downloadPct != null && (
        <p className="muted">Downloading… {downloadPct}%</p>
      )}

      {groups.map((group) => (
        <section key={group.id} className="model-library-group">
          <h4>{group.label}</h4>
          <div className="model-library-options">
            {group.options.map((opt) => (
              <article
                key={opt.id}
                className={`developer-card model-option ${
                  opt.active ? "is-active" : ""
                }`}
              >
                <span className="developer-card-label">
                  {opt.runtime === "onnx" ? "ONNX Runtime" : "Native"}
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
