import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type WheelEvent as ReactWheelEvent,
} from "react";

const MIN_SCALE = 1;
const MAX_SCALE = 8;
const ZOOM_STEP = 1.25;
const DOUBLE_CLICK_ZOOM = 2.5;

export type ViewerZoomState = {
  scale: number;
  tx: number;
  ty: number;
  isZoomed: boolean;
  zoomIn: () => void;
  zoomOut: () => void;
  reset: () => void;
  transformStyle: { transform: string; cursor: string };
  onWheel: (
    e: ReactWheelEvent<HTMLElement>,
    opts: { onPageNext: () => void; onPagePrev: () => void },
  ) => void;
  onDoubleClick: (e: React.MouseEvent<HTMLElement>) => void;
  onPointerDown: (e: ReactPointerEvent<HTMLElement>) => void;
  onPointerMove: (e: ReactPointerEvent<HTMLElement>) => void;
  onPointerUp: (e: ReactPointerEvent<HTMLElement>) => void;
};

function clampScale(s: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, s));
}

/**
 * Zoom / pan for the media lightbox. At 1×, wheel pages assets; when zoomed,
 * wheel zooms (Ctrl/⌘ or pinch) or pans and does not page.
 */
export function useViewerZoom(assetId: string): ViewerZoomState {
  const [scale, setScale] = useState(1);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);
  const dragRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    originTx: number;
    originTy: number;
  } | null>(null);
  const pageLockRef = useRef(0);

  useEffect(() => {
    setScale(1);
    setTx(0);
    setTy(0);
    dragRef.current = null;
  }, [assetId]);

  const reset = useCallback(() => {
    setScale(1);
    setTx(0);
    setTy(0);
  }, []);

  const zoomTo = useCallback((next: number, originX = 0, originY = 0) => {
    setScale((prev) => {
      const clamped = clampScale(next);
      if (clamped === 1) {
        setTx(0);
        setTy(0);
        return 1;
      }
      if (prev === clamped) return prev;
      // Keep the point under the cursor stable when zooming.
      const ratio = clamped / prev;
      setTx((x) => originX - (originX - x) * ratio);
      setTy((y) => originY - (originY - y) * ratio);
      return clamped;
    });
  }, []);

  const zoomIn = useCallback(() => {
    zoomTo(scale * ZOOM_STEP);
  }, [scale, zoomTo]);

  const zoomOut = useCallback(() => {
    zoomTo(scale / ZOOM_STEP);
  }, [scale, zoomTo]);

  const onWheel = useCallback(
    (
      e: ReactWheelEvent<HTMLElement>,
      opts: { onPageNext: () => void; onPagePrev: () => void },
    ) => {
      const wantsZoom = e.ctrlKey || e.metaKey || scale > 1;
      if (wantsZoom) {
        e.preventDefault();
        e.stopPropagation();
        if (e.ctrlKey || e.metaKey || Math.abs(e.deltaY) > 0) {
          const rect = e.currentTarget.getBoundingClientRect();
          const ox = e.clientX - rect.left - rect.width / 2;
          const oy = e.clientY - rect.top - rect.height / 2;
          // Trackpad pinch arrives as ctrl+wheel; deltaY < 0 zooms in.
          const factor = e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
          // Softer step for continuous trackpad deltas.
          const soft =
            Math.abs(e.deltaY) < 40
              ? Math.exp(-e.deltaY * 0.01)
              : factor;
          zoomTo(scale * soft, ox, oy);
        }
        return;
      }

      const delta =
        Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
      if (Math.abs(delta) < 12) return;
      const now = Date.now();
      if (now - pageLockRef.current < 260) return;
      pageLockRef.current = now;
      if (delta > 0) opts.onPageNext();
      else opts.onPagePrev();
    },
    [scale, zoomTo],
  );

  const onDoubleClick = useCallback(
    (e: React.MouseEvent<HTMLElement>) => {
      e.stopPropagation();
      if (scale > 1) {
        reset();
        return;
      }
      const rect = e.currentTarget.getBoundingClientRect();
      const ox = e.clientX - rect.left - rect.width / 2;
      const oy = e.clientY - rect.top - rect.height / 2;
      zoomTo(DOUBLE_CLICK_ZOOM, ox, oy);
    },
    [reset, scale, zoomTo],
  );

  const onPointerDown = useCallback(
    (e: ReactPointerEvent<HTMLElement>) => {
      if (scale <= 1 || e.button !== 0) return;
      // Let native video controls receive clicks.
      if ((e.target as HTMLElement).closest("video")) return;
      e.stopPropagation();
      e.currentTarget.setPointerCapture(e.pointerId);
      dragRef.current = {
        pointerId: e.pointerId,
        startX: e.clientX,
        startY: e.clientY,
        originTx: tx,
        originTy: ty,
      };
    },
    [scale, tx, ty],
  );

  const onPointerMove = useCallback((e: ReactPointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    e.stopPropagation();
    setTx(drag.originTx + (e.clientX - drag.startX));
    setTy(drag.originTy + (e.clientY - drag.startY));
  }, []);

  const onPointerUp = useCallback((e: ReactPointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    dragRef.current = null;
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
  }, []);

  const isZoomed = scale > 1;

  return {
    scale,
    tx,
    ty,
    isZoomed,
    zoomIn,
    zoomOut,
    reset,
    transformStyle: {
      transform: `translate(${tx}px, ${ty}px) scale(${scale})`,
      cursor: isZoomed ? "grab" : "zoom-in",
    },
    onWheel,
    onDoubleClick,
    onPointerDown,
    onPointerMove,
    onPointerUp,
  };
}
