import type {
  CSSProperties,
  DragEvent as ReactDragEvent,
  KeyboardEvent as ReactKeyboardEvent,
  MouseEvent as ReactMouseEvent,
} from "react";
import { Icon } from "../../components/icons";
import type { AssetSummary } from "../../lib/tauri";
import { AssetThumb } from "./AssetThumb";
import { CardMarks } from "./CardMarks";

/**
 * One selectable asset card: thumbnail, selection checkbox, favourite and
 * info buttons, video badge, and card marks. When `onOpen` is provided the
 * card itself is clickable (timeline grid); otherwise clicks are handled by
 * the surrounding grid's pointer/marquee logic (library grid).
 */
export function AssetCard({
  asset,
  isSelected,
  draggable,
  style,
  trashDays,
  onDragStart,
  onDragEnd,
  onToggleSelect,
  onToggleFavorite,
  onShowInfo,
  onOpen,
}: {
  asset: AssetSummary;
  isSelected: boolean;
  draggable: boolean;
  style?: CSSProperties;
  trashDays: number | null;
  onDragStart: (e: ReactDragEvent<HTMLDivElement>, asset: AssetSummary) => void;
  onDragEnd: () => void;
  onToggleSelect: (id: string) => void;
  onToggleFavorite: (asset: AssetSummary) => void;
  onShowInfo: (id: string) => void;
  onOpen?: (id: string) => void;
}) {
  const openProps = onOpen
    ? {
        role: "button",
        tabIndex: 0,
        "aria-label": "Open in viewer",
        onClick: (event: ReactMouseEvent<HTMLDivElement>) => {
          if (event.metaKey || event.ctrlKey || event.shiftKey) {
            onToggleSelect(asset.id);
          } else {
            onOpen(asset.id);
          }
        },
        onKeyDown: (event: ReactKeyboardEvent<HTMLDivElement>) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onOpen(asset.id);
          }
        },
      }
    : {};

  return (
    <div
      data-asset-id={asset.id}
      className={`asset-card ${isSelected ? "selected" : ""}`}
      style={style}
      draggable={draggable}
      onDragStart={(e) => onDragStart(e, asset)}
      onDragEnd={onDragEnd}
      {...openProps}
    >
      <AssetThumb asset={asset} />
      <button
        type="button"
        className={`check ${isSelected ? "on" : ""}`}
        role="checkbox"
        aria-checked={isSelected}
        aria-label={isSelected ? "Deselect photo" : "Select photo"}
        onClick={(e) => {
          e.stopPropagation();
          onToggleSelect(asset.id);
        }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        {isSelected ? "✓" : ""}
      </button>
      <button
        type="button"
        className={`fav-btn ${asset.favorite ? "on" : ""}`}
        title={asset.favorite ? "Remove favourite" : "Add to favourites"}
        onClick={(e) => {
          e.stopPropagation();
          onToggleFavorite(asset);
        }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        <Icon name={asset.favorite ? "heart" : "heartOutline"} />
      </button>
      <button
        type="button"
        className="info-btn"
        title="Media information"
        aria-label="Show media information"
        onClick={(e) => {
          e.stopPropagation();
          onShowInfo(asset.id);
        }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        <Icon name="info" />
      </button>
      {asset.mediaType === "video" && <span className="badge">Video</span>}
      <CardMarks asset={asset} trashDays={trashDays} />
    </div>
  );
}
