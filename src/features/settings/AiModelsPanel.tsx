import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { formatBytes } from "../../lib/format";
import {
  api,
  type EmbedProgress,
  type FacesProgress,
  type MlStatus,
  type ModelProgressEvent,
  type OcrProgress,
  type TagsProgress,
} from "../../lib/tauri";

const SEMANTIC_BUNDLE = "clip-vit-b32";
const OCR_BUNDLE = "rapidocr-ppv4";
const FACES_BUNDLE = "insightface-buffalo-l";
const TAGS_BUNDLE = "mobilenetv4-in1k";

/**
 * On-device intelligence controls: install CLIP / OCR / Faces bundles, watch
 * progress, and clear derived data. Lives under Settings → AI Features.
 */
export function AiModelsPanel() {
  const [status, setStatus] = useState<MlStatus | null>(null);
  const [progress, setProgress] = useState<EmbedProgress | null>(null);
  const [ocrProgress, setOcrProgress] = useState<OcrProgress | null>(null);
  const [facesProgress, setFacesProgress] = useState<FacesProgress | null>(null);
  const [tagsProgress, setTagsProgress] = useState<TagsProgress | null>(null);
  const [installingSemantic, setInstallingSemantic] = useState(false);
  const [installingOcr, setInstallingOcr] = useState(false);
  const [installingFaces, setInstallingFaces] = useState(false);
  const [installingTags, setInstallingTags] = useState(false);
  const [download, setDownload] = useState<ModelProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [ml, embed, ocr, faces, tags] = await Promise.all([
        api.mlStatus(),
        api.embedProgress(),
        api.ocrProgress(),
        api.facesProgress(),
        api.tagsProgress(),
      ]);
      setStatus(ml);
      setProgress(embed);
      setOcrProgress(ocr);
      setFacesProgress(faces);
      setTagsProgress(tags);
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

  async function installSemantic() {
    setInstallingSemantic(true);
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
      setInstallingSemantic(false);
      setDownload(null);
    }
  }

  async function installOcr() {
    setInstallingOcr(true);
    setError(null);
    setDownload(null);
    try {
      const next = await api.installOcrModels();
      setStatus(next);
      await api.kickOcr();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstallingOcr(false);
      setDownload(null);
    }
  }

  async function installFaces() {
    setInstallingFaces(true);
    setError(null);
    setDownload(null);
    try {
      const next = await api.installFaceModels();
      setStatus(next);
      await api.kickFaces();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstallingFaces(false);
      setDownload(null);
    }
  }

  async function installTags() {
    setInstallingTags(true);
    setError(null);
    setDownload(null);
    try {
      const next = await api.installTagsModels();
      setStatus(next);
      await api.kickTags();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstallingTags(false);
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

  async function clearOcrText() {
    if (
      !window.confirm(
        "Delete all extracted text?\n\nPhotos stay in your library. Text will be re-extracted in the background once OCR is enabled.",
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      await api.clearOcrText();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearFaceData() {
    if (
      !window.confirm(
        "Delete all face detections and people clusters?\n\nPhotos stay in your library. Faces will be re-detected once recognition is enabled.",
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      await api.clearFaceData();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearAutoTags() {
    if (
      !window.confirm(
        "Delete all auto-tags?\n\nPhotos stay in your library. Labels will be re-generated once object detection is enabled.",
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      await api.clearAutoTags();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function reprocess(kinds: Array<"semantic" | "ocr" | "faces" | "tags" | "all">) {
    const label =
      kinds[0] === "all"
        ? "semantic search, OCR, faces, and auto-tags"
        : kinds.join(", ");
    if (
      !window.confirm(
        `Clear derived ${label} data and re-run on the whole library?\n\nOriginals are never touched.`,
      )
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await api.reprocessAi(kinds);
      setError(
        `Reprocessing started — cleared ${result.embeddingsCleared} embeddings, ${result.ocrCleared} OCR rows, ${result.facesCleared} faces, ${result.tagsCleared} auto-tags.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function reclusterPeople() {
    setBusy(true);
    setError(null);
    try {
      const n = await api.reclusterFaces();
      setError(
        n > 0
          ? `Reclustered ${n} unnamed faces. Similar people were consolidated.`
          : "No unnamed faces to recluster.",
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeBundle(bundle: string, label: string) {
    if (
      !window.confirm(
        `Remove the ${label} models and their derived data?\n\nYou can download them again later. Your photo library is unaffected.`,
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      if (status) {
        for (const model of status.models) {
          if (model.bundle === bundle && model.installed) {
            await api.removeMlModel(model.id);
          }
        }
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const semanticReady = status?.semanticReady ?? false;
  const ocrReady = status?.ocrReady ?? false;
  const facesReady = status?.facesReady ?? false;
  const tagsReady = status?.tagsReady ?? false;
  const installing =
    installingSemantic || installingOcr || installingFaces || installingTags;
  const downloadPct =
    download && download.total > 0
      ? Math.min(100, Math.round((100 * download.downloaded) / download.total))
      : null;
  const anyReady = semanticReady || ocrReady || facesReady || tagsReady;

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
        <span className={`developer-health ${anyReady ? "" : "has-errors"}`}>
          {anyReady ? "Ready" : "Not installed"}
        </span>
      </header>

      {error && <p className="error-banner">{error}</p>}

      <article className="developer-card settings-ai-reprocess">
        <span className="developer-card-label">Re-run AI on the library</span>
        <strong>Clear derived data and rebuild</strong>
        <p className="muted">
          Use this after changing models, or when People groups look wrong.
          Originals stay untouched.
        </p>
        <div className="semantic-models-actions">
          <button
            type="button"
            onClick={() => void reprocess(["all"])}
            disabled={!anyReady || busy}
          >
            Re-run all
          </button>
          <button
            type="button"
            onClick={() => void reprocess(["semantic"])}
            disabled={!semanticReady || busy}
          >
            Semantic
          </button>
          <button
            type="button"
            onClick={() => void reprocess(["ocr"])}
            disabled={!ocrReady || busy}
          >
            OCR
          </button>
          <button
            type="button"
            onClick={() => void reprocess(["faces"])}
            disabled={!facesReady || busy}
          >
            Faces
          </button>
          <button
            type="button"
            onClick={() => void reprocess(["tags"])}
            disabled={!tagsReady || busy}
          >
            Auto-tags
          </button>
          <button
            type="button"
            onClick={() => void reclusterPeople()}
            disabled={!facesReady || busy}
          >
            Recluster people
          </button>
        </div>
      </article>

      <div className="semantic-models-grid">
        <article className="developer-card">
          <span className="developer-card-label">Semantic search</span>
          <strong>
            {semanticReady
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
              <dd>
                {formatBytes(
                  status?.models
                    .filter((m) => m.bundle === SEMANTIC_BUNDLE && m.installed)
                    .reduce((sum, m) => sum + m.sizeBytes, 0) ?? 0,
                )}
              </dd>
            </div>
            <div>
              <dt>Models folder</dt>
              <dd className="developer-path">{status?.modelsDir ?? "—"}</dd>
            </div>
          </dl>
          {!semanticReady ? (
            <button
              className="primary"
              onClick={() => void installSemantic()}
              disabled={installing || busy}
            >
              {installingSemantic
                ? downloadPct != null
                  ? `Downloading… ${downloadPct}%`
                  : "Downloading…"
                : "Download models"}
            </button>
          ) : (
            <button
              onClick={() =>
                void removeBundle(SEMANTIC_BUNDLE, "semantic search")
              }
              disabled={busy || installing}
            >
              Remove models
            </button>
          )}
          {installingSemantic && download && (
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
                {!semanticReady
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
              disabled={!semanticReady || busy}
            >
              Resume embedding
            </button>
            <button
              onClick={() => void clearEmbeddings()}
              disabled={!semanticReady || busy}
            >
              Clear embeddings
            </button>
          </div>
        </article>

        <article className="developer-card">
          <span className="developer-card-label">Text recognition (OCR)</span>
          <strong>
            {ocrReady
              ? "RapidOCR PP-OCRv4"
              : formatBytes(status?.ocrDownloadBytes ?? 0)}
          </strong>
          <dl>
            <div>
              <dt>License</dt>
              <dd>Apache-2.0</dd>
            </div>
            <div>
              <dt>Installed</dt>
              <dd>
                {formatBytes(
                  status?.models
                    .filter((m) => m.bundle === OCR_BUNDLE && m.installed)
                    .reduce((sum, m) => sum + m.sizeBytes, 0) ?? 0,
                )}
              </dd>
            </div>
            <div>
              <dt>Coverage</dt>
              <dd>
                {ocrProgress
                  ? `${ocrProgress.done} / ${ocrProgress.total}`
                  : "—"}
              </dd>
            </div>
          </dl>
          {!ocrReady ? (
            <button
              className="primary"
              onClick={() => void installOcr()}
              disabled={installing || busy}
            >
              {installingOcr
                ? downloadPct != null
                  ? `Downloading… ${downloadPct}%`
                  : "Downloading…"
                : "Download OCR models"}
            </button>
          ) : (
            <button
              onClick={() => void removeBundle(OCR_BUNDLE, "OCR")}
              disabled={busy || installing}
            >
              Remove models
            </button>
          )}
          {installingOcr && download && (
            <p className="muted semantic-download-detail">
              File {download.fileIndex}/{download.fileCount}: {download.modelId}
            </p>
          )}
        </article>

        <article className="developer-card">
          <span className="developer-card-label">Extracted text</span>
          <strong>
            {ocrProgress ? `${ocrProgress.done} / ${ocrProgress.total}` : "—"}
          </strong>
          <dl>
            <div>
              <dt>Status</dt>
              <dd>
                {!ocrReady
                  ? "Waiting for models"
                  : ocrProgress?.running
                    ? "Reading…"
                    : ocrProgress && ocrProgress.pending > 0
                      ? `${ocrProgress.pending} pending`
                      : "Up to date"}
              </dd>
            </div>
            {ocrProgress?.lastPath && (
              <div>
                <dt>Last</dt>
                <dd className="developer-path" title={ocrProgress.lastPath}>
                  {ocrProgress.lastPath}
                </dd>
              </div>
            )}
          </dl>
          <div className="semantic-models-actions">
            <button
              onClick={() => void api.kickOcr().then(refresh)}
              disabled={!ocrReady || busy}
            >
              Resume OCR
            </button>
            <button
              onClick={() => void clearOcrText()}
              disabled={!ocrReady || busy}
            >
              Clear text
            </button>
          </div>
        </article>

        <article className="developer-card">
          <span className="developer-card-label">Face recognition</span>
          <strong>
            {facesReady
              ? "InsightFace buffalo_l"
              : formatBytes(status?.facesDownloadBytes ?? 0)}
          </strong>
          <dl>
            <div>
              <dt>License</dt>
              <dd>InsightFace (non-commercial research)</dd>
            </div>
            <div>
              <dt>Installed</dt>
              <dd>
                {formatBytes(
                  status?.models
                    .filter((m) => m.bundle === FACES_BUNDLE && m.installed)
                    .reduce((sum, m) => sum + m.sizeBytes, 0) ?? 0,
                )}
              </dd>
            </div>
            <div>
              <dt>Coverage</dt>
              <dd>
                {facesProgress
                  ? `${facesProgress.done} / ${facesProgress.total}`
                  : "—"}
              </dd>
            </div>
          </dl>
          {!facesReady ? (
            <button
              className="primary"
              onClick={() => void installFaces()}
              disabled={installing || busy}
            >
              {installingFaces
                ? downloadPct != null
                  ? `Downloading… ${downloadPct}%`
                  : "Downloading…"
                : "Download face models"}
            </button>
          ) : (
            <button
              onClick={() => void removeBundle(FACES_BUNDLE, "face recognition")}
              disabled={busy || installing}
            >
              Remove models
            </button>
          )}
          {installingFaces && download && (
            <p className="muted semantic-download-detail">
              File {download.fileIndex}/{download.fileCount}: {download.modelId}
            </p>
          )}
        </article>

        <article className="developer-card">
          <span className="developer-card-label">Detected faces</span>
          <strong>
            {facesProgress
              ? `${facesProgress.done} / ${facesProgress.total}`
              : "—"}
          </strong>
          <dl>
            <div>
              <dt>Status</dt>
              <dd>
                {!facesReady
                  ? "Waiting for models"
                  : facesProgress?.running
                    ? "Detecting…"
                    : facesProgress && facesProgress.pending > 0
                      ? `${facesProgress.pending} pending`
                      : "Up to date"}
              </dd>
            </div>
            {facesProgress?.lastPath && (
              <div>
                <dt>Last</dt>
                <dd className="developer-path" title={facesProgress.lastPath}>
                  {facesProgress.lastPath}
                </dd>
              </div>
            )}
          </dl>
          <div className="semantic-models-actions">
            <button
              onClick={() => void api.kickFaces().then(refresh)}
              disabled={!facesReady || busy}
            >
              Resume faces
            </button>
            <button
              onClick={() => void clearFaceData()}
              disabled={!facesReady || busy}
            >
              Clear face data
            </button>
          </div>
        </article>

        <article className="developer-card">
          <span className="developer-card-label">Auto-tags (MobileNetV4)</span>
          <strong>
            {tagsReady
              ? "MobileNetV4 ImageNet"
              : formatBytes(status?.tagsDownloadBytes ?? 0)}
          </strong>
          <dl>
            <div>
              <dt>License</dt>
              <dd>Apache-2.0</dd>
            </div>
            <div>
              <dt>Installed</dt>
              <dd>
                {formatBytes(
                  status?.models
                    .filter((m) => m.bundle === TAGS_BUNDLE && m.installed)
                    .reduce((sum, m) => sum + m.sizeBytes, 0) ?? 0,
                )}
              </dd>
            </div>
            <div>
              <dt>Coverage</dt>
              <dd>
                {tagsProgress
                  ? `${tagsProgress.done} / ${tagsProgress.total}`
                  : "—"}
              </dd>
            </div>
          </dl>
          {!tagsReady ? (
            <button
              className="primary"
              onClick={() => void installTags()}
              disabled={installing || busy}
            >
              {installingTags
                ? downloadPct != null
                  ? `Downloading… ${downloadPct}%`
                  : "Downloading…"
                : "Download auto-tag models"}
            </button>
          ) : (
            <button
              onClick={() => void removeBundle(TAGS_BUNDLE, "auto-tags")}
              disabled={busy || installing}
            >
              Remove models
            </button>
          )}
          {installingTags && download && (
            <p className="muted semantic-download-detail">
              File {download.fileIndex}/{download.fileCount}: {download.modelId}
            </p>
          )}
        </article>

        <article className="developer-card">
          <span className="developer-card-label">Auto-tag progress</span>
          <strong>
            {tagsProgress ? `${tagsProgress.done} / ${tagsProgress.total}` : "—"}
          </strong>
          <dl>
            <div>
              <dt>Status</dt>
              <dd>
                {!tagsReady
                  ? "Waiting for models"
                  : tagsProgress?.running
                    ? "Tagging…"
                    : tagsProgress && tagsProgress.pending > 0
                      ? `${tagsProgress.pending} pending`
                      : "Up to date"}
              </dd>
            </div>
            {tagsProgress?.lastPath && (
              <div>
                <dt>Last</dt>
                <dd className="developer-path" title={tagsProgress.lastPath}>
                  {tagsProgress.lastPath}
                </dd>
              </div>
            )}
          </dl>
          <div className="semantic-models-actions">
            <button
              onClick={() => void api.kickTags().then(refresh)}
              disabled={!tagsReady || busy}
            >
              Resume auto-tags
            </button>
            <button
              onClick={() => void clearAutoTags()}
              disabled={!tagsReady || busy}
            >
              Clear auto-tags
            </button>
          </div>
        </article>
      </div>
    </div>
  );
}
