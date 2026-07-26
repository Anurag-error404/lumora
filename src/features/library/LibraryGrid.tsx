import type {
  DragEvent as ReactDragEvent,
  PointerEvent as ReactPointerEvent,
  RefObject,
} from "react";
import type { AssetSummary } from "../../lib/tauri";
import { trashDaysLeft } from "../../lib/labels";
import type { Marquee } from "../../types/app";
import { AssetCard } from "./AssetCard";
import { VirtualAssetGrid } from "./VirtualAssetGrid";

/** The main virtualised asset grid with marquee overlay and asset cards. */
export function LibraryGrid({
  assets,
  gridRef,
  onNearEnd,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  marquee,
  selected,
  isTrashView,
  trashRetentionDays,
  onAssetDragStart,
  onAssetDragEnd,
  onToggleSelect,
  onToggleFavorite,
  onShowInfo,
}: {
  assets: AssetSummary[];
  gridRef: RefObject<HTMLDivElement | null>;
  onNearEnd?: () => void;
  onPointerDown: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: ReactPointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: ReactPointerEvent<HTMLDivElement>) => void;
  marquee: Marquee | null;
  selected: Set<string>;
  isTrashView: boolean;
  trashRetentionDays: number;
  onAssetDragStart: (
    e: ReactDragEvent<HTMLDivElement>,
    asset: AssetSummary,
  ) => void;
  onAssetDragEnd: () => void;
  onToggleSelect: (id: string) => void;
  onToggleFavorite: (asset: AssetSummary) => void;
  onShowInfo: (id: string) => void;
}) {
  return (
    <VirtualAssetGrid
      assets={assets}
      gridRef={gridRef}
      onNearEnd={onNearEnd}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      overlay={
        marquee && (marquee.w > 2 || marquee.h > 2) ? (
          <div
            className="marquee"
            style={{
              left: marquee.x,
              top: marquee.y,
              width: marquee.w,
              height: marquee.h,
            }}
          />
        ) : null
      }
      renderItem={(asset, style) => (
        <AssetCard
          key={asset.id}
          asset={asset}
          isSelected={selected.has(asset.id)}
          draggable={!isTrashView && selected.has(asset.id)}
          style={style}
          trashDays={
            isTrashView
              ? trashDaysLeft(asset.deletedAt, trashRetentionDays)
              : null
          }
          onDragStart={onAssetDragStart}
          onDragEnd={onAssetDragEnd}
          onToggleSelect={onToggleSelect}
          onToggleFavorite={onToggleFavorite}
          onShowInfo={onShowInfo}
        />
      )}
    />
  );
}
