import { useMemo, useRef, useState, useEffect } from "react";
import {
  api,
  fileSrc,
  type AssetSummary,
  type EditResult,
  type EditSaveMode,
} from "../../lib/tauri";
import { VideoTrimTimeline } from "./VideoTrimTimeline";

/**
 * In-viewer video editor: trim via filmstrip timeline. Bakes via system ffmpeg
 * (stream-copy by default; re-encode when frame-accurate trim is enabled).
 */
export function VideoEditor({
  asset,
  onCancel,
  onSaved,
}: {
  asset: AssetSummary;
  onCancel: () => void;
  onSaved: (result: EditResult) => void;
}) {
  const durationHint = (asset.durationMs ?? 0) / 1000;
  const [duration, setDuration] = useState(
    durationHint > 0 ? durationHint : 1,
  );
  const [trimStart, setTrimStart] = useState(0);
  const [trimEnd, setTrimEnd] = useState(durationHint > 0 ? durationHint : 1);
  const [accurate, setAccurate] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ffmpegOk, setFfmpegOk] = useState(true);
  const [playhead, setPlayhead] = useState(0);

  const videoRef = useRef<HTMLVideoElement>(null);
  const src = fileSrc(asset.path);

  const dirty = useMemo(() => {
    const trimDirty =
      trimStart > 0.05 || Math.abs(trimEnd - duration) > 0.05;
    return trimDirty || accurate;
  }, [accurate, duration, trimEnd, trimStart]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const info = await api.probeVideoAsset(asset.id);
        if (cancelled) return;
        setFfmpegOk(info.ffmpegAvailable);
        if (info.durationMs && info.durationMs > 0) {
          const secs = info.durationMs / 1000;
          setDuration(secs);
          setTrimEnd(secs);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [asset.id]);

  function scrubPreview(time: number) {
    const video = videoRef.current;
    if (!video) return;
    const clamped = Math.min(
      Math.max(0, time),
      Math.max(0, (video.duration || duration) - 0.01),
    );
    setPlayhead(clamped);
    try {
      video.currentTime = clamped;
    } catch {
      /* seek may fail mid-load */
    }
  }

  function syncPlayheadFromVideo(video: HTMLVideoElement) {
    if (!Number.isFinite(video.currentTime)) return;
    setPlayhead(video.currentTime);
  }

  async function bake(mode: EditSaveMode) {
    if (busy) return;
    if (!ffmpegOk) {
      setError(
        "ffmpeg is not available. Install it (e.g. brew install ffmpeg) to edit videos.",
      );
      return;
    }
    if (trimEnd <= trimStart + 0.05) {
      setError("Trim end must be after trim start.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await api.applyVideoEdit(
        asset.id,
        {
          trimStart,
          trimEnd,
          crop: null,
          accurate,
        },
        mode,
      );
      onSaved(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const fileName = asset.path.split("/").pop() ?? asset.path;

  return (
    <div className="image-editor video-editor" role="dialog" aria-label="Edit video">
      <header className="image-editor-bar">
        <div>
          <strong>Edit video</strong>
          <span className="muted"> · {fileName}</span>
        </div>
        <div>
          <button type="button" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
        </div>
      </header>

      <div className="image-editor-stage">
        <div className="image-editor-preview">
          <video
            ref={videoRef}
            className="viewer-video"
            src={src ?? undefined}
            controls
            playsInline
            preload="metadata"
            onLoadedMetadata={(e) => {
              const v = e.currentTarget;
              if (v.duration && Number.isFinite(v.duration)) {
                setDuration(v.duration);
                setTrimEnd((end) =>
                  end <= 0 || end > v.duration ? v.duration : end,
                );
              }
              syncPlayheadFromVideo(v);
            }}
            onTimeUpdate={(e) => syncPlayheadFromVideo(e.currentTarget)}
            onSeeked={(e) => syncPlayheadFromVideo(e.currentTarget)}
            onPlay={(e) => syncPlayheadFromVideo(e.currentTarget)}
          />
        </div>
      </div>

      <div className="image-editor-controls video-editor-controls">
        <section className="video-editor-trim-section">
          <div className="video-editor-trim-head">
            <h3>Trim</h3>
            <button
              type="button"
              disabled={busy}
              onClick={() => {
                setTrimStart(0);
                setTrimEnd(duration);
                scrubPreview(0);
              }}
            >
              Reset
            </button>
          </div>

          <VideoTrimTimeline
            src={src}
            duration={duration}
            trimStart={trimStart}
            trimEnd={trimEnd}
            currentTime={playhead}
            disabled={busy}
            onChange={(start, end) => {
              setTrimStart(start);
              setTrimEnd(end);
            }}
            onScrub={scrubPreview}
          />

          <label className="video-editor-check">
            <input
              type="checkbox"
              checked={accurate}
              disabled={busy}
              onChange={(e) => setAccurate(e.target.checked)}
            />
            <span>Frame-accurate trim (re-encode)</span>
          </label>
          <p className="muted video-editor-hint">
            {accurate
              ? "Will re-encode (H.264 / AAC, CRF 18)."
              : "Uses stream copy; cut may snap to the nearest keyframe. Drag the handles to set in/out."}
          </p>
        </section>
      </div>

      {error && <p className="image-editor-error">{error}</p>}
      {!ffmpegOk && (
        <p className="image-editor-error">
          ffmpeg not found on PATH. Video editing requires ffmpeg.
        </p>
      )}

      <footer className="image-editor-actions">
        <button
          type="button"
          className="primary"
          disabled={busy || !dirty || !ffmpegOk}
          onClick={() => void bake("replace")}
        >
          {busy ? "Saving…" : "Replace original"}
        </button>
        <button
          type="button"
          disabled={busy || !dirty || !ffmpegOk}
          onClick={() => void bake("copy")}
        >
          Save as copy
        </button>
      </footer>
    </div>
  );
}
