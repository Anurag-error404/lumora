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
  type CaptionsProgress,
} from "../../lib/tauri";
import type { PrefsUpdater } from "./settingsUi";

const SEMANTIC_BUNDLE = "clip-vit-b32";
const OCR_BUNDLE = "paddleocr-ppv5";
const FACES_BUNDLE = "insightface-buffalo-l";
const TAGS_BUNDLE = "mobilenetv4-in1k";
const CAPTIONS_BUNDLE = "florence-2-base-ft";
const PROSE_BUNDLE = "lamini-flan-t5-248m";

type PipelineKind = "embed" | "ocr" | "faces" | "tags" | "captions";

type PipelineStatus =
  | "waiting"
  | "running"
  | "paused"
  | "pending"
  | "failed"
  | "done";

function pipelineStatus(opts: {
  ready: boolean;
  running: boolean;
  paused: boolean;
  pending: number;
  failed: number;
}): PipelineStatus {
  if (!opts.ready) return "waiting";
  if (opts.paused) return "paused";
  if (opts.running) return "running";
  if (opts.pending > 0) return "pending";
  if (opts.failed > 0) return "failed";
  return "done";
}

function statusLabel(
  status: PipelineStatus,
  pending: number,
  failed: number,
): string {
  switch (status) {
    case "waiting":
      return "Waiting for models";
    case "running":
      return "Running";
    case "paused":
      return pending + failed > 0
        ? `Paused · ${pending + failed} left`
        : "Paused";
    case "pending":
      return failed > 0
        ? `${pending} pending · ${failed} failed`
        : `${pending} pending`;
    case "failed":
      return `${failed} failed`;
    case "done":
      return "Up to date";
  }
}

function progressPct(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.round((100 * done) / total));
}

function fileName(path: string | null | undefined): string | null {
  if (!path) return null;
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

/**
 * On-device intelligence controls: install CLIP / OCR / Faces bundles, watch
 * progress, and clear derived data. Lives under Settings → AI Features.
 */
export function AiModelsPanel({ updatePrefs }: { updatePrefs: PrefsUpdater }) {
  const [status, setStatus] = useState<MlStatus | null>(null);
  const [progress, setProgress] = useState<EmbedProgress | null>(null);
  const [ocrProgress, setOcrProgress] = useState<OcrProgress | null>(null);
  const [facesProgress, setFacesProgress] = useState<FacesProgress | null>(null);
  const [tagsProgress, setTagsProgress] = useState<TagsProgress | null>(null);
  const [captionsProgress, setCaptionsProgress] = useState<CaptionsProgress | null>(null);
  const [installingSemantic, setInstallingSemantic] = useState(false);
  const [installingOcr, setInstallingOcr] = useState(false);
  const [installingFaces, setInstallingFaces] = useState(false);
  const [installingTags, setInstallingTags] = useState(false);
  const [installingCaptions, setInstallingCaptions] = useState(false);
  const [installingProse, setInstallingProse] = useState(false);
  const [download, setDownload] = useState<ModelProgressEvent | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [ml, embed, ocr, faces, tags, captions] = await Promise.all([
        api.mlStatus(),
        api.embedProgress(),
        api.ocrProgress(),
        api.facesProgress(),
        api.tagsProgress(),
        api.captionsProgress(),
      ]);
      setStatus(ml);
      setProgress(embed);
      setOcrProgress(ocr);
      setFacesProgress(faces);
      setTagsProgress(tags);
      setCaptionsProgress(captions);
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

  async function installCaptions() {
    setInstallingCaptions(true);
    setError(null);
    setDownload(null);
    try {
      setStatus(await api.installCaptionsModels());
      await api.kickCaptions();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstallingCaptions(false);
      setDownload(null);
    }
  }

  async function installProse() {
    setInstallingProse(true);
    setError(null);
    setDownload(null);
    try {
      setStatus(await api.installProseModels());
      await updatePrefs((prefs) => {
        prefs.ai.memoryProse = true;
        return prefs;
      });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setInstallingProse(false);
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

  async function clearCaptions() {
    if (!window.confirm("Delete all image captions?\n\nPhotos stay in your library. Captions will be regenerated once enabled.")) return;
    setBusy(true);
    try {
      await api.clearCaptions();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function clearMemoryProse() {
    if (
      !window.confirm(
        "Delete cached memory prose?\n\nOpening a memory will regenerate lines when memory prose is enabled.",
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      await api.clearMemoryProse();
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function reprocess(kinds: Array<"semantic" | "ocr" | "faces" | "tags" | "captions" | "all">) {
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

  async function setPipelinePaused(kind: PipelineKind, paused: boolean) {
    setBusy(true);
    try {
      if (paused) {
        switch (kind) {
          case "embed":
            await api.pauseEmbedding();
            break;
          case "ocr":
            await api.pauseOcr();
            break;
          case "faces":
            await api.pauseFaces();
            break;
          case "tags":
            await api.pauseTags();
            break;
          case "captions":
            await api.pauseCaptions();
            break;
        }
      } else {
        // Workers no-op when their feature toggle is off or global AI is paused.
        await ensurePipelineEnabled(kind);
        switch (kind) {
          case "embed":
            await api.kickEmbedding();
            break;
          case "ocr":
            await api.kickOcr();
            break;
          case "faces":
            await api.kickFaces();
            break;
          case "tags":
            await api.kickTags();
            break;
          case "captions":
            await api.kickCaptions();
            break;
        }
      }
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function ensurePipelineEnabled(kind: PipelineKind) {
    await updatePrefs((prefs) => {
      if (prefs.ai.backgroundProcessing === "paused") {
        prefs.ai.backgroundProcessing = "always";
      }
      if (kind === "ocr") prefs.ai.ocr = true;
      if (kind === "faces") prefs.ai.faceRecognition = true;
      if (kind === "tags") prefs.ai.objectDetection = true;
      if (kind === "captions") prefs.ai.captions = true;
      if (kind === "embed") prefs.ai.semanticSearch = true;
      return prefs;
    });
  }

  const semanticReady = status?.semanticReady ?? false;
  const ocrReady = status?.ocrReady ?? false;
  const facesReady = status?.facesReady ?? false;
  const tagsReady = status?.tagsReady ?? false;
  const captionsReady = status?.captionsReady ?? false;
  const proseReady = status?.proseReady ?? false;
  const installing =
    installingSemantic ||
    installingOcr ||
    installingFaces ||
    installingTags ||
    installingCaptions ||
    installingProse;
  const downloadPct =
    download && download.total > 0
      ? Math.min(100, Math.round((100 * download.downloaded) / download.total))
      : null;
  const anyReady =
    semanticReady || ocrReady || facesReady || tagsReady || captionsReady || proseReady;

  const installedBytes = (bundle: string) =>
    formatBytes(
      status?.models
        .filter((m) => m.bundle === bundle && m.installed)
        .reduce((sum, m) => sum + m.sizeBytes, 0) ?? 0,
    );

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
            onClick={() => void reprocess(["captions"])}
            disabled={!captionsReady || busy}
          >
            Captions
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

      <div className="ai-pipeline-list">
        <AiPipelineRow
          modelLabel="Semantic search"
          modelName={
            semanticReady
              ? "CLIP ViT-B/32"
              : formatBytes(status?.semanticDownloadBytes ?? 0)
          }
          license="MIT"
          installed={installedBytes(SEMANTIC_BUNDLE)}
          modelsDir={status?.modelsDir}
          ready={semanticReady}
          installing={installingSemantic}
          downloadPct={installingSemantic ? downloadPct : null}
          download={installingSemantic ? download : null}
          installLabel="Download models"
          onInstall={() => void installSemantic()}
          onRemove={() => void removeBundle(SEMANTIC_BUNDLE, "semantic search")}
          installDisabled={installing || busy}
          removeDisabled={busy || installing}
          progressLabel="Library embeddings"
          done={progress?.embedded ?? 0}
          total={progress?.total ?? 0}
          pending={progress?.pending ?? 0}
          failed={progress?.failed ?? 0}
          running={progress?.running ?? false}
          paused={progress?.paused ?? false}
          lastPath={progress?.lastPath ?? null}
          lastError={progress?.lastError ?? null}
          onPause={() => void setPipelinePaused("embed", true)}
          onResume={() => void setPipelinePaused("embed", false)}
          onClear={() => void clearEmbeddings()}
          clearLabel="Clear embeddings"
          busy={busy}
        />
        <AiPipelineRow
          modelLabel="Image captions (Florence-2)"
          modelName={captionsReady ? "Florence-2 Base" : formatBytes(status?.captionsDownloadBytes ?? 0)}
          license="MIT"
          installed={installedBytes(CAPTIONS_BUNDLE)}
          ready={captionsReady}
          installing={installingCaptions}
          downloadPct={installingCaptions ? downloadPct : null}
          download={installingCaptions ? download : null}
          installLabel="Download caption models"
          onInstall={() => void installCaptions()}
          onRemove={() => void removeBundle(CAPTIONS_BUNDLE, "image captions")}
          installDisabled={installing || busy}
          removeDisabled={busy || installing}
          progressLabel="Image captions"
          done={captionsProgress?.done ?? 0}
          total={captionsProgress?.total ?? 0}
          pending={captionsProgress?.pending ?? 0}
          failed={captionsProgress?.failed ?? 0}
          running={captionsProgress?.running ?? false}
          paused={captionsProgress?.paused ?? false}
          lastPath={captionsProgress?.lastPath ?? null}
          lastError={captionsProgress?.lastError ?? null}
          onPause={() => void setPipelinePaused("captions", true)}
          onResume={() => void setPipelinePaused("captions", false)}
          onClear={() => void clearCaptions()}
          clearLabel="Clear captions"
          busy={busy}
        />

        <AiPipelineRow
          modelLabel="Memory prose (LaMini-Flan-T5)"
          modelName={
            proseReady
              ? "LaMini-Flan-T5 248M"
              : formatBytes(status?.proseDownloadBytes ?? 0)
          }
          license="CC-BY-NC-4.0"
          installed={installedBytes(PROSE_BUNDLE)}
          ready={proseReady}
          installing={installingProse}
          downloadPct={installingProse ? downloadPct : null}
          download={installingProse ? download : null}
          installLabel="Download prose model"
          onInstall={() => void installProse()}
          onRemove={() => void removeBundle(PROSE_BUNDLE, "memory prose")}
          installDisabled={installing || busy}
          removeDisabled={busy || installing}
          progressLabel="Memory prose"
          done={0}
          total={0}
          pending={0}
          failed={0}
          running={false}
          paused={false}
          lastPath={null}
          lastError={null}
          onPause={() => undefined}
          onResume={() => undefined}
          onClear={() => void clearMemoryProse()}
          clearLabel="Clear cached prose"
          busy={busy}
          onDemandNote="Runs when you open a memory — no library-wide queue."
        />

        <AiPipelineRow
          modelLabel="Text recognition (OCR)"
          modelName={
            ocrReady
              ? "PaddleOCR PP-OCRv5"
              : formatBytes(status?.ocrDownloadBytes ?? 0)
          }
          license="Apache-2.0"
          installed={installedBytes(OCR_BUNDLE)}
          ready={ocrReady}
          installing={installingOcr}
          downloadPct={installingOcr ? downloadPct : null}
          download={installingOcr ? download : null}
          installLabel="Download OCR models"
          onInstall={() => void installOcr()}
          onRemove={() => void removeBundle(OCR_BUNDLE, "OCR")}
          installDisabled={installing || busy}
          removeDisabled={busy || installing}
          progressLabel="Extracted text"
          done={ocrProgress?.done ?? 0}
          total={ocrProgress?.total ?? 0}
          pending={ocrProgress?.pending ?? 0}
          failed={ocrProgress?.failed ?? 0}
          running={ocrProgress?.running ?? false}
          paused={ocrProgress?.paused ?? false}
          lastPath={ocrProgress?.lastPath ?? null}
          lastError={ocrProgress?.lastError ?? null}
          onPause={() => void setPipelinePaused("ocr", true)}
          onResume={() => void setPipelinePaused("ocr", false)}
          onClear={() => void clearOcrText()}
          clearLabel="Clear text"
          busy={busy}
        />

        <AiPipelineRow
          modelLabel="Face recognition"
          modelName={
            facesReady
              ? "InsightFace buffalo_l"
              : formatBytes(status?.facesDownloadBytes ?? 0)
          }
          license="InsightFace (non-commercial research)"
          installed={installedBytes(FACES_BUNDLE)}
          ready={facesReady}
          installing={installingFaces}
          downloadPct={installingFaces ? downloadPct : null}
          download={installingFaces ? download : null}
          installLabel="Download face models"
          onInstall={() => void installFaces()}
          onRemove={() =>
            void removeBundle(FACES_BUNDLE, "face recognition")
          }
          installDisabled={installing || busy}
          removeDisabled={busy || installing}
          progressLabel="Detected faces"
          done={facesProgress?.done ?? 0}
          total={facesProgress?.total ?? 0}
          pending={facesProgress?.pending ?? 0}
          failed={facesProgress?.failed ?? 0}
          running={facesProgress?.running ?? false}
          paused={facesProgress?.paused ?? false}
          lastPath={facesProgress?.lastPath ?? null}
          lastError={facesProgress?.lastError ?? null}
          onPause={() => void setPipelinePaused("faces", true)}
          onResume={() => void setPipelinePaused("faces", false)}
          onClear={() => void clearFaceData()}
          clearLabel="Clear face data"
          busy={busy}
        />

        <AiPipelineRow
          modelLabel="Auto-tags (MobileNetV4)"
          modelName={
            tagsReady
              ? "MobileNetV4 ImageNet"
              : formatBytes(status?.tagsDownloadBytes ?? 0)
          }
          license="Apache-2.0"
          installed={installedBytes(TAGS_BUNDLE)}
          ready={tagsReady}
          installing={installingTags}
          downloadPct={installingTags ? downloadPct : null}
          download={installingTags ? download : null}
          installLabel="Download auto-tag models"
          onInstall={() => void installTags()}
          onRemove={() => void removeBundle(TAGS_BUNDLE, "auto-tags")}
          installDisabled={installing || busy}
          removeDisabled={busy || installing}
          progressLabel="Auto-tag progress"
          done={tagsProgress?.done ?? 0}
          total={tagsProgress?.total ?? 0}
          pending={tagsProgress?.pending ?? 0}
          failed={tagsProgress?.failed ?? 0}
          running={tagsProgress?.running ?? false}
          paused={tagsProgress?.paused ?? false}
          lastPath={tagsProgress?.lastPath ?? null}
          lastError={tagsProgress?.lastError ?? null}
          onPause={() => void setPipelinePaused("tags", true)}
          onResume={() => void setPipelinePaused("tags", false)}
          onClear={() => void clearAutoTags()}
          clearLabel="Clear auto-tags"
          busy={busy}
        />
      </div>
    </div>
  );
}

function AiPipelineRow({
  modelLabel,
  modelName,
  license,
  installed,
  modelsDir,
  ready,
  installing,
  downloadPct,
  download,
  installLabel,
  onInstall,
  onRemove,
  installDisabled,
  removeDisabled,
  progressLabel,
  done,
  total,
  pending,
  failed,
  running,
  paused,
  lastPath,
  lastError,
  onPause,
  onResume,
  onClear,
  clearLabel,
  busy,
  onDemandNote,
}: {
  modelLabel: string;
  modelName: string;
  license: string;
  installed: string;
  modelsDir?: string;
  ready: boolean;
  installing: boolean;
  downloadPct: number | null;
  download: ModelProgressEvent | null;
  installLabel: string;
  onInstall: () => void;
  onRemove: () => void;
  installDisabled: boolean;
  removeDisabled: boolean;
  progressLabel: string;
  done: number;
  total: number;
  pending: number;
  failed: number;
  running: boolean;
  paused: boolean;
  lastPath: string | null;
  lastError: string | null;
  onPause: () => void;
  onResume: () => void;
  onClear: () => void;
  clearLabel: string;
  busy: boolean;
  /** When set, hide library progress and show this note instead. */
  onDemandNote?: string;
}) {
  const status = pipelineStatus({ ready, running, paused, pending, failed });
  const pct = progressPct(done, total);
  const incomplete = pending > 0 || failed > 0;
  // Hide Pause/Resume once the library is fully processed (including failures cleared).
  const showPause = ready && pending > 0 && running && !paused;
  const showResume = ready && incomplete && (paused || !running);
  const resumeLabel =
    failed > 0 && pending === 0 ? "Retry failed" : "Resume";
  const currentFile = fileName(lastPath);

  return (
    <section className={`ai-pipeline-row is-${status}`}>
      <article className="developer-card ai-pipeline-model">
        <span className="developer-card-label">{modelLabel}</span>
        <strong>{modelName}</strong>
        <dl>
          <div>
            <dt>License</dt>
            <dd>{license}</dd>
          </div>
          <div>
            <dt>Installed</dt>
            <dd>{installed}</dd>
          </div>
          {modelsDir && (
            <div>
              <dt>Models folder</dt>
              <dd className="developer-path">{modelsDir}</dd>
            </div>
          )}
        </dl>
        {!ready ? (
          <button
            type="button"
            className="primary"
            onClick={onInstall}
            disabled={installDisabled}
          >
            {installing
              ? downloadPct != null
                ? `Downloading… ${downloadPct}%`
                : "Downloading…"
              : installLabel}
          </button>
        ) : (
          <button type="button" onClick={onRemove} disabled={removeDisabled}>
            Remove models
          </button>
        )}
        {installing && download && (
          <p className="muted semantic-download-detail">
            File {download.fileIndex}/{download.fileCount}: {download.modelId}
          </p>
        )}
      </article>

      <article className="developer-card ai-pipeline-progress">
        <div className="ai-pipeline-progress-head">
          <span className="developer-card-label">{progressLabel}</span>
          <span className={`ai-pipeline-badge is-${status}`} aria-live="polite">
            {onDemandNote
              ? ready
                ? "On demand"
                : "Waiting"
              : statusLabel(status, pending, failed)}
          </span>
        </div>

        {onDemandNote ? (
          <p className="muted">{onDemandNote}</p>
        ) : (
          <>
            <div className="ai-pipeline-count">
              <strong>{ready ? `${done} / ${total}` : "—"}</strong>
              {ready && total > 0 && <span className="muted">{pct}%</span>}
            </div>

            <div
              className={`ai-pipeline-track ${status === "running" ? "is-active" : ""}`}
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={ready ? pct : 0}
              aria-label={progressLabel}
            >
              <div
                className="ai-pipeline-fill"
                style={{ width: ready ? `${pct}%` : "0%" }}
              />
            </div>

            {status === "running" && currentFile && (
              <p className="ai-pipeline-file muted" title={lastPath ?? undefined}>
                {currentFile}
              </p>
            )}
            {status === "paused" && currentFile && (
              <p className="ai-pipeline-file muted" title={lastPath ?? undefined}>
                Last: {currentFile}
              </p>
            )}
            {lastError && (status === "failed" || status === "pending" || failed > 0) && (
              <p className="ai-pipeline-error" title={lastError} role="status">
                {lastError}
              </p>
            )}
          </>
        )}

        <div className="ai-pipeline-actions">
          {!onDemandNote && showPause && (
            <button
              type="button"
              className="primary"
              onClick={onPause}
              disabled={busy}
            >
              Pause
            </button>
          )}
          {!onDemandNote && showResume && (
            <button
              type="button"
              className="primary"
              onClick={onResume}
              disabled={busy}
            >
              {resumeLabel}
            </button>
          )}
          <button
            type="button"
            className="danger"
            onClick={onClear}
            disabled={!ready || busy}
          >
            {clearLabel}
          </button>
        </div>
      </article>
    </section>
  );
}
