import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";

const FRAME_COUNT = 24;
const MIN_TRIM_SEC = 0.1;
const HANDLE_HIT_PX = 14;

function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  const frac = Math.floor((seconds % 1) * 10);
  return `${m}:${String(s).padStart(2, "0")}.${frac}`;
}

function waitForSeek(video: HTMLVideoElement): Promise<void> {
  return new Promise((resolve, reject) => {
    const onSeeked = () => {
      cleanup();
      resolve();
    };
    const onError = () => {
      cleanup();
      reject(new Error("video seek failed"));
    };
    const cleanup = () => {
      video.removeEventListener("seeked", onSeeked);
      video.removeEventListener("error", onError);
    };
    video.addEventListener("seeked", onSeeked);
    video.addEventListener("error", onError);
  });
}

async function sampleFilmstrip(
  src: string,
  duration: number,
  count: number,
  signal: { cancelled: boolean },
): Promise<string[]> {
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.preload = "auto";
  video.src = src;

  await new Promise<void>((resolve, reject) => {
    const onMeta = () => {
      cleanup();
      resolve();
    };
    const onErr = () => {
      cleanup();
      reject(new Error("could not load video for filmstrip"));
    };
    const cleanup = () => {
      video.removeEventListener("loadedmetadata", onMeta);
      video.removeEventListener("error", onErr);
    };
    video.addEventListener("loadedmetadata", onMeta);
    video.addEventListener("error", onErr);
  });

  if (signal.cancelled) return [];

  const dur = Number.isFinite(video.duration) && video.duration > 0
    ? video.duration
    : duration;
  const vw = video.videoWidth || 320;
  const vh = video.videoHeight || 180;
  const thumbH = 56;
  const thumbW = Math.max(32, Math.round((thumbH * vw) / Math.max(1, vh)));

  const canvas = document.createElement("canvas");
  canvas.width = thumbW;
  canvas.height = thumbH;
  const ctx = canvas.getContext("2d");
  if (!ctx) return [];

  const frames: string[] = [];
  const n = Math.max(2, count);
  for (let i = 0; i < n; i++) {
    if (signal.cancelled) return frames;
    const t = n === 1 ? 0 : (i / (n - 1)) * Math.max(0, dur - 0.05);
    video.currentTime = Math.min(Math.max(0, t), Math.max(0, dur - 0.01));
    try {
      await waitForSeek(video);
    } catch {
      break;
    }
    if (signal.cancelled) return frames;
    ctx.fillStyle = "#111";
    ctx.fillRect(0, 0, thumbW, thumbH);
    ctx.drawImage(video, 0, 0, thumbW, thumbH);
    frames.push(canvas.toDataURL("image/jpeg", 0.72));
  }

  video.removeAttribute("src");
  video.load();
  return frames;
}

type DragKind = "start" | "end" | "window";

/**
 * Filmstrip trim timeline: sampled video frames with draggable in/out handles.
 */
export function VideoTrimTimeline({
  src,
  duration,
  trimStart,
  trimEnd,
  currentTime = 0,
  disabled,
  onChange,
  onScrub,
}: {
  src: string | null;
  duration: number;
  trimStart: number;
  trimEnd: number;
  /** Playhead position in seconds (synced to the preview video). */
  currentTime?: number;
  disabled?: boolean;
  onChange: (start: number, end: number) => void;
  onScrub?: (time: number) => void;
}) {
  const trackRef = useRef<HTMLDivElement>(null);
  const [frames, setFrames] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const dragRef = useRef<{
    kind: DragKind;
    originX: number;
    start0: number;
    end0: number;
  } | null>(null);

  useEffect(() => {
    if (!src || duration <= 0) {
      setFrames([]);
      return;
    }
    const signal = { cancelled: false };
    setLoading(true);
    setFailed(false);
    void sampleFilmstrip(src, duration, FRAME_COUNT, signal)
      .then((shots) => {
        if (signal.cancelled) return;
        setFrames(shots);
        setFailed(shots.length === 0);
      })
      .catch(() => {
        if (!signal.cancelled) {
          setFrames([]);
          setFailed(true);
        }
      })
      .finally(() => {
        if (!signal.cancelled) setLoading(false);
      });
    return () => {
      signal.cancelled = true;
    };
  }, [src, duration]);

  const clampPair = useCallback(
    (start: number, end: number) => {
      const dur = Math.max(MIN_TRIM_SEC, duration);
      let s = Math.min(Math.max(0, start), dur - MIN_TRIM_SEC);
      let e = Math.min(Math.max(s + MIN_TRIM_SEC, end), dur);
      if (e - s < MIN_TRIM_SEC) {
        if (s <= 0) e = MIN_TRIM_SEC;
        else s = e - MIN_TRIM_SEC;
      }
      return { start: s, end: e };
    },
    [duration],
  );

  const timeFromClientX = useCallback(
    (clientX: number) => {
      const el = trackRef.current;
      if (!el || duration <= 0) return 0;
      const rect = el.getBoundingClientRect();
      const ratio = (clientX - rect.left) / Math.max(1, rect.width);
      return Math.min(Math.max(0, ratio), 1) * duration;
    },
    [duration],
  );

  const onPointerDown = (kind: DragKind) => (e: ReactPointerEvent) => {
    if (disabled || duration <= 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragRef.current = {
      kind,
      originX: e.clientX,
      start0: trimStart,
      end0: trimEnd,
    };
  };

  const applyDrag = useCallback(
    (clientX: number) => {
      const drag = dragRef.current;
      const el = trackRef.current;
      if (!drag || !el) return;
      const rect = el.getBoundingClientRect();
      const dxSec =
        ((clientX - drag.originX) / Math.max(1, rect.width)) * duration;

      if (drag.kind === "start") {
        const next = clampPair(drag.start0 + dxSec, drag.end0);
        onChange(next.start, next.end);
        onScrub?.(next.start);
      } else if (drag.kind === "end") {
        const next = clampPair(drag.start0, drag.end0 + dxSec);
        onChange(next.start, next.end);
        onScrub?.(next.end);
      } else {
        const span = drag.end0 - drag.start0;
        let s = drag.start0 + dxSec;
        s = Math.min(Math.max(0, s), duration - span);
        onChange(s, s + span);
        onScrub?.(s);
      }
    },
    [clampPair, duration, onChange, onScrub],
  );

  const onPointerMove = (e: ReactPointerEvent) => {
    if (!dragRef.current) return;
    applyDrag(e.clientX);
  };

  const onPointerUp = (e: ReactPointerEvent) => {
    if (!dragRef.current) return;
    dragRef.current = null;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
  };

  const onTrackPointerUp = (e: ReactPointerEvent) => {
    // Only scrub on a click (no active drag). Handles/selection stopPropagation.
    if (disabled || dragRef.current) return;
    const t = timeFromClientX(e.clientX);
    onScrub?.(t);
  };

  const startPct = duration > 0 ? (trimStart / duration) * 100 : 0;
  const endPct = duration > 0 ? (trimEnd / duration) * 100 : 100;
  const widthPct = Math.max(0, endPct - startPct);
  const playheadPct =
    duration > 0
      ? Math.min(100, Math.max(0, (currentTime / duration) * 100))
      : 0;

  return (
    <div className="video-trim-timeline">
      <div className="video-trim-meta">
        <span>
          In <strong>{formatTime(trimStart)}</strong>
        </span>
        <span className="muted">
          Selection {formatTime(Math.max(0, trimEnd - trimStart))}
        </span>
        <span>
          Out <strong>{formatTime(trimEnd)}</strong>
        </span>
      </div>

      <div
        ref={trackRef}
        className={`video-trim-track${disabled ? " is-disabled" : ""}`}
        role="group"
        aria-label="Video trim timeline"
        onPointerMove={onPointerMove}
        onPointerUp={(e) => {
          onPointerUp(e);
          onTrackPointerUp(e);
        }}
        onPointerCancel={onPointerUp}
      >
        <div className="video-trim-filmstrip" aria-hidden="true">
          {loading && frames.length === 0 && (
            <div className="video-trim-filmstrip-placeholder">
              Loading frames…
            </div>
          )}
          {failed && frames.length === 0 && !loading && (
            <div className="video-trim-filmstrip-placeholder">
              Frame preview unavailable
            </div>
          )}
          {frames.map((frame, i) => (
            <img
              key={i}
              src={frame}
              alt=""
              draggable={false}
              className="video-trim-frame"
            />
          ))}
          {!loading && frames.length === 0 && !failed && (
            <div className="video-trim-filmstrip-placeholder muted">
              No frames
            </div>
          )}
        </div>

        <div
          className="video-trim-shade video-trim-shade-left"
          style={{ width: `${startPct}%` }}
        />
        <div
          className="video-trim-shade video-trim-shade-right"
          style={{ width: `${100 - endPct}%` }}
        />

        <div
          className="video-trim-playhead"
          style={{ left: `${playheadPct}%` }}
          aria-hidden="true"
        />

        <div
          className="video-trim-selection"
          style={{ left: `${startPct}%`, width: `${widthPct}%` }}
          onPointerDown={onPointerDown("window")}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        >
          <button
            type="button"
            className="video-trim-handle video-trim-handle-start"
            aria-label={`Trim start ${formatTime(trimStart)}`}
            disabled={disabled}
            style={{ touchAction: "none", width: HANDLE_HIT_PX }}
            onPointerDown={onPointerDown("start")}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
          />
          <button
            type="button"
            className="video-trim-handle video-trim-handle-end"
            aria-label={`Trim end ${formatTime(trimEnd)}`}
            disabled={disabled}
            style={{ touchAction: "none", width: HANDLE_HIT_PX }}
            onPointerDown={onPointerDown("end")}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
          />
        </div>
      </div>
    </div>
  );
}
