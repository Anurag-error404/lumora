import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import {
  api,
  fileSrc,
  type AssetSummary,
  type CropRect,
  type EditOps,
  type EditResult,
  type EditSaveMode,
} from "../../lib/tauri";

type CropBox = { x: number; y: number; width: number; height: number };

type AspectPreset = {
  id: string;
  label: string;
  /** null = free; "original" = image aspect; else w/h */
  ratio: number | "original" | null;
};

const ASPECT_PRESETS: AspectPreset[] = [
  { id: "free", label: "Free", ratio: null },
  { id: "original", label: "Original", ratio: "original" },
  { id: "1:1", label: "1:1", ratio: 1 },
  { id: "4:3", label: "4:3", ratio: 4 / 3 },
  { id: "3:4", label: "3:4", ratio: 3 / 4 },
  { id: "16:9", label: "16:9", ratio: 16 / 9 },
  { id: "9:16", label: "9:16", ratio: 9 / 16 },
  { id: "3:2", label: "3:2", ratio: 3 / 2 },
  { id: "2:3", label: "2:3", ratio: 2 / 3 },
];

const FULL_CROP: CropBox = { x: 0, y: 0, width: 1, height: 1 };
const MIN_CROP = 0.08;

type Handle = "move" | "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw";

function clampCrop(box: CropBox): CropBox {
  let { x, y, width, height } = box;
  width = Math.max(MIN_CROP, Math.min(1, width));
  height = Math.max(MIN_CROP, Math.min(1, height));
  x = Math.min(Math.max(0, x), 1 - width);
  y = Math.min(Math.max(0, y), 1 - height);
  return { x, y, width, height };
}

function fitAspect(
  imageAspect: number,
  target: number | "original" | null,
): CropBox {
  if (target === null) return { ...FULL_CROP };
  const ratio = target === "original" ? imageAspect : target;
  // Crop box is in normalized image space (square 0–1 × 0–1 mapping to pixels).
  // Desired pixel aspect = (width * imgW) / (height * imgH) = ratio
  // → width/height = ratio / imageAspect  (normalized)
  const normRatio = ratio / imageAspect;
  let width: number;
  let height: number;
  if (normRatio >= 1) {
    width = 1;
    height = 1 / normRatio;
  } else {
    height = 1;
    width = normRatio;
  }
  return clampCrop({
    x: (1 - width) / 2,
    y: (1 - height) / 2,
    width,
    height,
  });
}

function cropIsFull(c: CropBox): boolean {
  return (
    c.x <= 0.001 &&
    c.y <= 0.001 &&
    c.width >= 0.999 &&
    c.height >= 0.999
  );
}

function toApiCrop(c: CropBox): CropRect | null {
  if (cropIsFull(c)) return null;
  return { x: c.x, y: c.y, width: c.width, height: c.height };
}

function orientedAspect(
  naturalW: number,
  naturalH: number,
  rotate: number,
): number {
  const swap = rotate === 90 || rotate === 270;
  const w = swap ? naturalH : naturalW;
  const h = swap ? naturalW : naturalH;
  return w / Math.max(1, h);
}

/** In-viewer image editor: rotate, flip, aspect crop, exposure. */
export function ImageEditor({
  asset,
  onCancel,
  onSaved,
}: {
  asset: AssetSummary;
  onCancel: () => void;
  onSaved: (result: EditResult) => void;
}) {
  const [rotate, setRotate] = useState(0);
  const [flipH, setFlipH] = useState(false);
  const [flipV, setFlipV] = useState(false);
  const [exposure, setExposure] = useState(0);
  const [crop, setCrop] = useState<CropBox>(FULL_CROP);
  const [aspectId, setAspectId] = useState("free");
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [imgFailed, setImgFailed] = useState(false);

  const stageRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);
  const [frame, setFrame] = useState({ left: 0, top: 0, width: 0, height: 0 });

  const dragRef = useRef<{
    handle: Handle;
    startX: number;
    startY: number;
    origin: CropBox;
    frameW: number;
    frameH: number;
  } | null>(null);

  const imageAspect = useMemo(() => {
    if (!natural) return 1;
    return orientedAspect(natural.w, natural.h, rotate);
  }, [natural, rotate]);

  const activeAspect = ASPECT_PRESETS.find((p) => p.id === aspectId)?.ratio ?? null;

  const dirty =
    rotate !== 0 ||
    flipH ||
    flipV ||
    exposure !== 0 ||
    !cropIsFull(crop);

  const previewFilter = useMemo(() => {
    const brightness = Math.pow(2, exposure);
    return `brightness(${brightness})`;
  }, [exposure]);

  // CSS applies right→left: rotate first, then flips (matches backend order).
  const previewTransform = `scaleX(${flipH ? -1 : 1}) scaleY(${flipV ? -1 : 1}) rotate(${rotate}deg)`;

  const syncFrame = useCallback(() => {
    const stage = stageRef.current;
    const img = imgRef.current;
    if (!stage || !img) return;
    const sr = stage.getBoundingClientRect();
    const ir = img.getBoundingClientRect();
    setFrame({
      left: ir.left - sr.left,
      top: ir.top - sr.top,
      width: ir.width,
      height: ir.height,
    });
  }, []);

  useEffect(() => {
    syncFrame();
    const stage = stageRef.current;
    if (!stage) return;
    const ro = new ResizeObserver(() => syncFrame());
    ro.observe(stage);
    window.addEventListener("resize", syncFrame);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", syncFrame);
    };
  }, [syncFrame, rotate, flipH, flipV, natural]);

  const applyDrag = useCallback(
    (clientX: number, clientY: number) => {
      const drag = dragRef.current;
      if (!drag || drag.frameW <= 0) return;
      const dx = (clientX - drag.startX) / drag.frameW;
      const dy = (clientY - drag.startY) / drag.frameH;
      const o = drag.origin;
      let next = { ...o };

      const resize = (edges: {
        n?: boolean;
        s?: boolean;
        e?: boolean;
        w?: boolean;
      }) => {
        let x = o.x;
        let y = o.y;
        let w = o.width;
        let h = o.height;
        if (edges.e) w = o.width + dx;
        if (edges.s) h = o.height + dy;
        if (edges.w) {
          x = o.x + dx;
          w = o.width - dx;
        }
        if (edges.n) {
          y = o.y + dy;
          h = o.height - dy;
        }

        if (activeAspect !== null && natural) {
          const target =
            activeAspect === "original" ? imageAspect : activeAspect;
          const normRatio = target / imageAspect;
          if ((edges.e || edges.w) && !(edges.n || edges.s)) {
            h = w / normRatio;
            y = o.y + (o.height - h) / 2;
          } else if ((edges.n || edges.s) && !(edges.e || edges.w)) {
            w = h * normRatio;
            x = o.x + (o.width - w) / 2;
          } else {
            h = w / normRatio;
            if (edges.n) y = o.y + o.height - h;
            if (edges.w) x = o.x + o.width - w;
          }
        }

        next = clampCrop({ x, y, width: w, height: h });
      };

      switch (drag.handle) {
        case "move":
          next = clampCrop({
            x: o.x + dx,
            y: o.y + dy,
            width: o.width,
            height: o.height,
          });
          break;
        case "n":
          resize({ n: true });
          break;
        case "s":
          resize({ s: true });
          break;
        case "e":
          resize({ e: true });
          break;
        case "w":
          resize({ w: true });
          break;
        case "ne":
          resize({ n: true, e: true });
          break;
        case "nw":
          resize({ n: true, w: true });
          break;
        case "se":
          resize({ s: true, e: true });
          break;
        case "sw":
          resize({ s: true, w: true });
          break;
      }
      setCrop(next);
    },
    [activeAspect, imageAspect, natural],
  );

  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      if (!dragRef.current) return;
      applyDrag(e.clientX, e.clientY);
    };
    const onUp = () => {
      dragRef.current = null;
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [applyDrag]);

  function applyAspect(id: string) {
    setAspectId(id);
    const preset = ASPECT_PRESETS.find((p) => p.id === id);
    if (!preset || !natural) return;
    setCrop(fitAspect(imageAspect, preset.ratio));
  }

  function onRotate(delta: number) {
    const next = (rotate + delta + 360) % 360;
    setRotate(next);
    const preset = ASPECT_PRESETS.find((p) => p.id === aspectId);
    if (preset && natural && preset.ratio !== null) {
      const nextAspect = orientedAspect(natural.w, natural.h, next);
      setCrop(fitAspect(nextAspect, preset.ratio));
    }
    requestAnimationFrame(syncFrame);
  }

  function onCropPointerDown(e: ReactPointerEvent, handle: Handle) {
    if (busy || frame.width <= 0) return;
    e.preventDefault();
    e.stopPropagation();
    dragRef.current = {
      handle,
      startX: e.clientX,
      startY: e.clientY,
      origin: { ...crop },
      frameW: frame.width,
      frameH: frame.height,
    };
  }

  async function save(mode: EditSaveMode) {
    if (!dirty) {
      setError("Make a change before saving");
      return;
    }
    if (mode === "replace") {
      if (
        !window.confirm(
          "Replace the original file on disk?\n\nThis cannot be undone. Prefer “Save as copy” if you want to keep the original.",
        )
      ) {
        return;
      }
    }
    setBusy(true);
    setError(null);
    try {
      const ops: EditOps = {
        rotateDegrees: rotate,
        flipHorizontal: flipH,
        flipVertical: flipV,
        crop: toApiCrop(crop),
        exposure,
      };
      const result = await api.applyImageEdit(asset.id, ops, mode);
      onSaved(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const fileName = asset.path.split(/[/\\]/).pop() ?? asset.path;
  const cropStyle = {
    left: `${crop.x * 100}%`,
    top: `${crop.y * 100}%`,
    width: `${crop.width * 100}%`,
    height: `${crop.height * 100}%`,
  };

  return (
    <div
      className="image-editor"
      role="dialog"
      aria-modal="true"
      aria-label={`Edit ${fileName}`}
      onClick={(e) => e.stopPropagation()}
    >
      <header className="image-editor-bar">
        <div>
          <strong>Edit photo</strong>
          <span className="muted">{fileName}</span>
        </div>
        <button type="button" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
      </header>

      <div className="image-editor-stage" ref={stageRef}>
        <div
          className="image-editor-preview"
          style={{
            transform: previewTransform,
            filter: previewFilter,
          }}
        >
          {imgFailed ? (
            <div className="image-editor-missing">Could not load image</div>
          ) : (
            <img
              ref={imgRef}
              src={fileSrc(asset.path) ?? undefined}
              alt={fileName}
              draggable={false}
              onLoad={(e) => {
                const el = e.currentTarget;
                setNatural({ w: el.naturalWidth, h: el.naturalHeight });
                requestAnimationFrame(syncFrame);
              }}
              onError={() => setImgFailed(true)}
            />
          )}
        </div>

        {frame.width > 0 && (
          <div
            className="image-editor-crop-layer"
            style={{
              left: frame.left,
              top: frame.top,
              width: frame.width,
              height: frame.height,
            }}
          >
            <div
              className="image-editor-crop-box"
              style={cropStyle}
              onPointerDown={(e) => onCropPointerDown(e, "move")}
            >
              <span className="image-editor-crop-rule" />
              <span className="image-editor-crop-rule horiz" />
              {(
                [
                  ["n", "n"],
                  ["s", "s"],
                  ["e", "e"],
                  ["w", "w"],
                  ["ne", "ne"],
                  ["nw", "nw"],
                  ["se", "se"],
                  ["sw", "sw"],
                ] as const
              ).map(([cls, handle]) => (
                <button
                  key={handle}
                  type="button"
                  className={`image-editor-crop-handle ${cls}`}
                  aria-label={`Resize crop ${handle}`}
                  disabled={busy}
                  onPointerDown={(e) => onCropPointerDown(e, handle)}
                />
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="image-editor-controls">
        <section>
          <h3>Orient</h3>
          <div className="image-editor-row">
            <button
              type="button"
              disabled={busy}
              onClick={() => onRotate(270)}
              title="Rotate left"
            >
              ⟲ 90°
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => onRotate(90)}
              title="Rotate right"
            >
              90° ⟳
            </button>
            <button
              type="button"
              className={flipH ? "primary" : undefined}
              disabled={busy}
              onClick={() => setFlipH((v) => !v)}
              title="Flip horizontal"
            >
              ↔ Flip H
            </button>
            <button
              type="button"
              className={flipV ? "primary" : undefined}
              disabled={busy}
              onClick={() => setFlipV((v) => !v)}
              title="Flip vertical"
            >
              ↕ Flip V
            </button>
          </div>
          <div className="image-editor-row">
            <button
              type="button"
              disabled={busy || (rotate === 0 && !flipH && !flipV)}
              onClick={() => {
                setRotate(0);
                setFlipH(false);
                setFlipV(false);
              }}
            >
              Reset orientation
            </button>
            <span className="muted">{rotate}°</span>
          </div>
        </section>

        <section>
          <h3>Crop aspect</h3>
          <div className="image-editor-aspects" role="group" aria-label="Aspect ratio">
            {ASPECT_PRESETS.map((p) => (
              <button
                key={p.id}
                type="button"
                className={aspectId === p.id ? "primary" : undefined}
                disabled={busy || (!natural && p.ratio !== null)}
                onClick={() => applyAspect(p.id)}
              >
                {p.label}
              </button>
            ))}
          </div>
          <div className="image-editor-row">
            <button
              type="button"
              disabled={busy || cropIsFull(crop)}
              onClick={() => {
                setAspectId("free");
                setCrop(FULL_CROP);
              }}
            >
              Reset crop
            </button>
            <span className="muted">Drag the box · pull corners to resize</span>
          </div>
        </section>

        <section>
          <h3>Exposure</h3>
          <label className="image-editor-slider">
            <span>
              Stops
              <span className="muted">
                {exposure > 0 ? `+${exposure.toFixed(1)}` : exposure.toFixed(1)}{" "}
                EV
              </span>
            </span>
            <input
              type="range"
              min={-2}
              max={2}
              step={0.1}
              value={exposure}
              disabled={busy}
              onChange={(e) => setExposure(Number(e.target.value))}
            />
          </label>
          <button
            type="button"
            disabled={busy || exposure === 0}
            onClick={() => setExposure(0)}
          >
            Reset exposure
          </button>
        </section>
      </div>

      {error && <p className="error-banner image-editor-error">{error}</p>}

      <footer className="image-editor-actions">
        <button
          type="button"
          className="danger"
          disabled={busy || !dirty}
          onClick={() => void save("replace")}
        >
          {busy ? "Saving…" : "Save (replace original)"}
        </button>
        <button
          type="button"
          className="primary"
          disabled={busy || !dirty}
          onClick={() => void save("copy")}
        >
          {busy ? "Saving…" : "Save as copy"}
        </button>
      </footer>
    </div>
  );
}
