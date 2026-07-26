import {
  useEffect,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type RefObject,
} from "react";
import type { AssetSummary } from "../../lib/tauri";

type VirtualLayout = {
  cols: number;
  cell: number;
  gap: number;
  start: number;
  end: number;
  height: number;
};

/**
 * Windowed grid: only the rows near the viewport are mounted, so the DOM
 * stays small no matter how many assets are loaded. Pointer/marquee handlers
 * attach to the container exactly like the old CSS grid did.
 */
export function VirtualAssetGrid({
  assets,
  gridRef,
  onNearEnd,
  overlay,
  renderItem,
  onPointerDown,
  onPointerMove,
  onPointerUp,
}: {
  assets: AssetSummary[];
  gridRef: RefObject<HTMLDivElement | null>;
  onNearEnd?: () => void;
  overlay?: ReactNode;
  renderItem: (asset: AssetSummary, style: CSSProperties) => ReactNode;
  onPointerDown: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: ReactPointerEvent<HTMLDivElement>) => void;
}) {
  const [layout, setLayout] = useState<VirtualLayout>({
    cols: 1,
    cell: 160,
    gap: 6,
    start: 0,
    end: 0,
    height: 0,
  });

  useEffect(() => {
    const el = gridRef.current;
    if (!el) return;
    const scroller = el.closest(".content") as HTMLElement | null;

    const recompute = () => {
      const width = el.clientWidth;
      if (width <= 0) return;
      const styles = getComputedStyle(el);
      const thumb = parseFloat(styles.getPropertyValue("--thumb")) || 160;
      const gap = parseFloat(styles.getPropertyValue("--grid-gap")) || 6;
      const cols = Math.max(1, Math.floor((width + gap) / (thumb + gap)));
      const cell = (width - gap * (cols - 1)) / cols;
      const rowH = cell + gap;
      const rows = Math.ceil(assets.length / cols);
      const height = rows > 0 ? rows * rowH - gap : 0;

      let start = 0;
      let end = assets.length;
      if (scroller) {
        const gridTop =
          el.getBoundingClientRect().top -
          scroller.getBoundingClientRect().top +
          scroller.scrollTop;
        const viewTop = scroller.scrollTop - gridTop;
        const viewH = scroller.clientHeight;
        const OVERSCAN_ROWS = 4;
        const startRow = Math.max(
          0,
          Math.floor(viewTop / rowH) - OVERSCAN_ROWS,
        );
        const endRow = Math.ceil((viewTop + viewH) / rowH) + OVERSCAN_ROWS;
        start = Math.min(assets.length, startRow * cols);
        end = Math.min(assets.length, endRow * cols);
        if (
          onNearEnd &&
          scroller.scrollTop + viewH >= gridTop + height - viewH * 2
        ) {
          onNearEnd();
        }
      }

      setLayout((prev) =>
        prev.cols === cols &&
        prev.cell === cell &&
        prev.gap === gap &&
        prev.start === start &&
        prev.end === end &&
        prev.height === height
          ? prev
          : { cols, cell, gap, start, end, height },
      );
    };

    recompute();
    const observer = new ResizeObserver(recompute);
    observer.observe(el);
    scroller?.addEventListener("scroll", recompute, { passive: true });
    return () => {
      observer.disconnect();
      scroller?.removeEventListener("scroll", recompute);
    };
  }, [assets.length, gridRef, onNearEnd]);

  const { cols, cell, gap, start, end, height } = layout;

  return (
    <div
      className="asset-grid virtual"
      ref={gridRef}
      style={{ height }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    >
      {overlay}
      {assets.slice(start, end).map((asset, i) => {
        const index = start + i;
        return renderItem(asset, {
          position: "absolute",
          left: (index % cols) * (cell + gap),
          top: Math.floor(index / cols) * (cell + gap),
          width: cell,
          height: cell,
        });
      })}
    </div>
  );
}
