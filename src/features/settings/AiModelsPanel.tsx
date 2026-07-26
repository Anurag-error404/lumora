import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { formatBytes } from "../../lib/format";
import {
  api,
  type EmbedProgress,
  type MlStatus,
  type ModelProgressEvent,
} from "../../lib/tauri";

/**
 * On-device intelligence controls: install the CLIP bundle, watch embedding
 * progress, and clear derived data. Lives under Settings → AI Features.
 */
export function AiModelsPanel() {
  const [status, setStatus] = useState<MlStatus | null>(null);
  const [progress, setProgress] = useState<EmbedProgress | null>(null);
  const [installing, setInstalling] = useState(false);
  const [download, setDownload] = useState<ModelProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [ml, embed] = await Promise.all([
        api.mlStatus(),
        api.embedProgress(),
      ]);
      setStatus(ml);
      setProgress(embed);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), 2500);
    return () => window.clearInterval(id);
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

  async function install() {
    setInstalling(true);
    setError(null);
    setDownload(null);
    try {
      const next = await api.installSemanticModels();
      setStatus(next);
      await api.kickEmbedding();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstalling(false);
      setDownload(null);
    }
  }

  async function clearEmbeddings() {
    if (
      !window.confirm(
        "Delete all CLIP embeddings?\n\nPhotos stay in your library. They will be re-embedded in the background once you confirm.",
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      await api.clearMlEmbeddings();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeBundle() {
    if (
      !window.confirm(
        "Remove the semantic search models and all embeddings?\n\nYou can download them again later. Your photo library is unaffected.",
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      if (status) {
        for (const model of status.models) {
          if (model.installed) await api.removeMlModel(model.id);
        }
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const ready = status?.semanticReady ?? false;
  const downloadPct =
    download && download.total > 0
      ? Math.min(100, Math.round((100 * download.downloaded) / download.total))
      : null;

  return (
    <div className="settings-ai-models">
      <header className="settings-block-head">
        <div>
          <h3>AI Models</h3>
          <p className="muted">
            Models run entirely on your Mac. Nothing downloads until you ask —
            never in the background.
          </p>
        </div>
        <span className={`developer-health ${ready ? "" : "has-errors"}`}>
          {ready ? "Ready" : "Not installed"}
        </span>
      </header>

      {error && <p className="error-banner">{error}</p>}

      <div className="semantic-models-grid">
        <article className="developer-card">
          <span className="developer-card-label">Semantic search</span>
          <strong>
            {ready
              ? "CLIP ViT-B/32"
              : formatBytes(status?.semanticDownloadBytes ?? 0)}
          </strong>
          <dl>
            <div>
              <dt>License</dt>
              <dd>MIT</dd>
            </div>
            <div>
              <dt>Installed</dt>
              <dd>{formatBytes(status?.installedBytes ?? 0)}</dd>
            </div>
            <div>
              <dt>Models folder</dt>
              <dd className="developer-path">{status?.modelsDir ?? "—"}</dd>
            </div>
          </dl>
          {!ready ? (
            <button
              className="primary"
              onClick={() => void install()}
              disabled={installing || busy}
            >
              {installing
                ? downloadPct != null
                  ? `Downloading… ${downloadPct}%`
                  : "Downloading…"
                : "Download models"}
            </button>
          ) : (
            <button onClick={() => void removeBundle()} disabled={busy || installing}>
              Remove models
            </button>
          )}
          {installing && download && (
            <p className="muted semantic-download-detail">
              File {download.fileIndex}/{download.fileCount}: {download.modelId}
            </p>
          )}
        </article>

        <article className="developer-card">
          <span className="developer-card-label">Library embeddings</span>
          <strong>
            {progress ? `${progress.embedded} / ${progress.total}` : "—"}
          </strong>
          <dl>
            <div>
              <dt>Status</dt>
              <dd>
                {!ready
                  ? "Waiting for models"
                  : progress?.running
                    ? "Embedding…"
                    : progress && progress.pending > 0
                      ? `${progress.pending} pending`
                      : "Up to date"}
              </dd>
            </div>
            {progress?.lastPath && (
              <div>
                <dt>Last</dt>
                <dd className="developer-path" title={progress.lastPath}>
                  {progress.lastPath}
                </dd>
              </div>
            )}
          </dl>
          <div className="semantic-models-actions">
            <button
              onClick={() => void api.kickEmbedding().then(refresh)}
              disabled={!ready || busy}
            >
              Resume embedding
            </button>
            <button onClick={() => void clearEmbeddings()} disabled={!ready || busy}>
              Clear embeddings
            </button>
          </div>
        </article>
      </div>
    </div>
  );
}
