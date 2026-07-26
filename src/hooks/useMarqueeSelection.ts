import {
  useRef,
  useState,
  type Dispatch,
  type PointerEvent as ReactPointerEvent,
  type SetStateAction,
} from "react";
import type { AlbumPickTarget, Marquee } from "../types/app";

/**
 * Grid pointer handling: plain click opens the viewer (or selects when
 * `doubleClickOpensViewer` is on), modifier clicks and the checkbox select,
 * and dragging draws a marquee over the asset cards.
 */
export function useMarqueeSelection({
  selected,
  setSelected,
  pickingForAlbum,
  onOpenAsset,
  onToggleAsset,
  doubleClickOpensViewer = false,
}: {
  selected: Set<string>;
  setSelected: Dispatch<SetStateAction<Set<string>>>;
  pickingForAlbum: AlbumPickTarget | null;
  onOpenAsset: (id: string) => void;
  onToggleAsset: (id: string) => void;
  doubleClickOpensViewer?: boolean;
}) {
  const gridRef = useRef<HTMLDivElement>(null);
  const [marquee, setMarquee] = useState<Marquee | null>(null);

  const dragOrigin = useRef<{
    x: number;
    y: number;
    additive: boolean;
    replace: boolean;
    pointerId: number;
    hitAssetId: string | null;
  } | null>(null);
  const selectionAtDragStart = useRef<Set<string>>(new Set());
  const marqueeRef = useRef<Marquee | null>(null);
  const didMarqueeRef = useRef(false);
  const lastClickRef = useRef<{ id: string; at: number } | null>(null);

  function idsIntersectingMarquee(rect: Marquee): string[] {
    const root = gridRef.current;
    if (!root) return [];
    const rootBox = root.getBoundingClientRect();
    const abs = {
      left: rootBox.left + rect.x,
      top: rootBox.top + rect.y,
      right: rootBox.left + rect.x + rect.w,
      bottom: rootBox.top + rect.y + rect.h,
    };
    const hits: string[] = [];
    root.querySelectorAll<HTMLElement>("[data-asset-id]").forEach((el) => {
      const box = el.getBoundingClientRect();
      const overlap =
        box.left < abs.right &&
        box.right > abs.left &&
        box.top < abs.bottom &&
        box.bottom > abs.top;
      if (overlap) {
        const id = el.dataset.assetId;
        if (id) hits.push(id);
      }
    });
    return hits;
  }

  function onGridPointerDown(e: ReactPointerEvent<HTMLDivElement>) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest(".fav-btn") || target.closest("button")) return;
    if (!gridRef.current?.contains(target)) return;

    const root = gridRef.current;
    const box = root.getBoundingClientRect();
    const x = e.clientX - box.left;
    const y = e.clientY - box.top;
    const additive = e.metaKey || e.ctrlKey || e.shiftKey;
    const replace = e.altKey;
    const card = target.closest<HTMLElement>("[data-asset-id]");
    dragOrigin.current = {
      x,
      y,
      additive,
      replace,
      pointerId: e.pointerId,
      hitAssetId: card?.dataset.assetId ?? null,
    };
    // Marquee baseline: additive modifiers keep prior selection; plain marquee starts fresh
    selectionAtDragStart.current = additive ? new Set(selected) : new Set();
    didMarqueeRef.current = false;
    marqueeRef.current = { x, y, w: 0, h: 0 };
    setMarquee(null);
    root.setPointerCapture(e.pointerId);
  }

  function onGridPointerMove(e: ReactPointerEvent<HTMLDivElement>) {
    const origin = dragOrigin.current;
    if (!origin || origin.pointerId !== e.pointerId) return;
    const root = gridRef.current;
    if (!root) return;
    const box = root.getBoundingClientRect();
    const x = e.clientX - box.left;
    const y = e.clientY - box.top;
    const rect: Marquee = {
      x: Math.min(origin.x, x),
      y: Math.min(origin.y, y),
      w: Math.abs(x - origin.x),
      h: Math.abs(y - origin.y),
    };
    marqueeRef.current = rect;

    // Require a real drag before treating this as marquee selection
    if (rect.w > 6 || rect.h > 6) {
      didMarqueeRef.current = true;
      setMarquee(rect);
      const hits = idsIntersectingMarquee(rect);
      const next = origin.additive
        ? new Set(selectionAtDragStart.current)
        : new Set<string>();
      for (const id of hits) next.add(id);
      setSelected(next);
    }
  }

  function onGridPointerUp(e: ReactPointerEvent<HTMLDivElement>) {
    const origin = dragOrigin.current;
    if (!origin || origin.pointerId !== e.pointerId) return;
    const root = gridRef.current;
    const wasMarquee = didMarqueeRef.current;
    const rect = marqueeRef.current;
    dragOrigin.current = null;
    marqueeRef.current = null;
    didMarqueeRef.current = false;
    setMarquee(null);
    try {
      root?.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }

    // Plain click: open viewer, or select when double-click-to-open is enabled.
    // Selection stays available through the card checkbox, Cmd/Shift click,
    // Alt click, and marquee drag.
    // (Don't use e.target here — pointer capture makes it the grid.)
    if (!wasMarquee) {
      const id = origin.hitAssetId;
      if (!id) {
        if (!origin.additive) setSelected(new Set());
        lastClickRef.current = null;
        return;
      }
      if (origin.replace) {
        setSelected(new Set([id]));
        lastClickRef.current = null;
      } else if (origin.additive || pickingForAlbum) {
        // While picking photos for an album, a plain click keeps picking.
        onToggleAsset(id);
        lastClickRef.current = null;
      } else if (doubleClickOpensViewer) {
        const now = Date.now();
        const prev = lastClickRef.current;
        if (prev && prev.id === id && now - prev.at < 400) {
          onOpenAsset(id);
          lastClickRef.current = null;
        } else {
          setSelected(new Set([id]));
          lastClickRef.current = { id, at: now };
        }
      } else {
        onOpenAsset(id);
        lastClickRef.current = null;
      }
      return;
    }

    // Marquee: without Shift/Cmd replace with hits; with modifier add to prior selection
    if (rect) {
      const hits = idsIntersectingMarquee(rect);
      const next = origin.additive
        ? new Set(selectionAtDragStart.current)
        : new Set<string>();
      for (const id of hits) next.add(id);
      setSelected(next);
    }
  }

  /** Abort any in-flight marquee tracking (e.g. when native DnD takes over). */
  function cancelMarquee() {
    dragOrigin.current = null;
    marqueeRef.current = null;
    didMarqueeRef.current = false;
    setMarquee(null);
  }

  return {
    gridRef,
    marquee,
    onGridPointerDown,
    onGridPointerMove,
    onGridPointerUp,
    cancelMarquee,
  };
}
