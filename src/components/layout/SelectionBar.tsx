import { useState } from "react";
import { IconButton } from "../icons";
import { LABEL_COLORS } from "../../lib/constants";

/**
 * Action bar shown while photos are selected: favourites, tagging, export,
 * albums, trash, and a "more" menu with rating and colour labels.
 */
export function SelectionBar({
  selectedCount,
  isTrashView,
  busy,
  onClearSelection,
  onRestore,
  onPermanentDelete,
  onFavorite,
  onOpenTagModal,
  onExportZip,
  onOpenMoveAlbum,
  onMoveToLocked,
  onDelete,
  onSelectAllVisible,
  onRate,
  onLabel,
}: {
  selectedCount: number;
  isTrashView: boolean;
  busy: boolean;
  onClearSelection: () => void;
  onRestore: () => void;
  onPermanentDelete: (deleteFiles: boolean) => void;
  onFavorite: (favorite: boolean) => void;
  onOpenTagModal: () => void;
  onExportZip: () => void;
  onOpenMoveAlbum: () => void;
  onMoveToLocked: () => void;
  onDelete: () => void;
  onSelectAllVisible: () => void;
  onRate: (rating: number) => void;
  onLabel: (color: string | null) => void;
}) {
  const [moreOpen, setMoreOpen] = useState(false);

  return (
    <div className="selection-bar">
      <IconButton
        icon="close"
        label="Clear selection"
        onClick={onClearSelection}
      />
      <span className="selection-count">{selectedCount} selected</span>
      <div className="spacer" />

      {isTrashView ? (
        <>
          <IconButton
            icon="history"
            label="Restore to library"
            onClick={onRestore}
          />
          <IconButton
            icon="trash"
            label="Delete files from disk…"
            danger
            onClick={() => onPermanentDelete(true)}
          />
        </>
      ) : (
        <>
          <IconButton
            icon="star"
            label="Add to favourites"
            onClick={() => onFavorite(true)}
          />
          <IconButton
            icon="label"
            label="Tag selection…"
            onClick={onOpenTagModal}
          />
          <IconButton
            icon="download"
            label="Export as ZIP…"
            onClick={onExportZip}
            disabled={busy}
          />
          <IconButton
            icon="album"
            label="Move to album…"
            onClick={onOpenMoveAlbum}
          />
          <IconButton
            icon="lock"
            label="Move to Locked folder"
            onClick={onMoveToLocked}
            disabled={busy}
          />
          <IconButton
            icon="trash"
            label="Move to trash"
            onClick={onDelete}
          />
        </>
      )}

      <div className="menu-anchor">
        <IconButton
          icon="more"
          label="More actions"
          onClick={() => setMoreOpen((open) => !open)}
        />
        {moreOpen && (
          <>
            <div className="menu-backdrop" onClick={() => setMoreOpen(false)} />
            <div className="menu" role="menu">
              <button
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  onSelectAllVisible();
                }}
              >
                Select all visible
              </button>
              {isTrashView ? (
                <button
                  role="menuitem"
                  onClick={() => {
                    setMoreOpen(false);
                    onPermanentDelete(false);
                  }}
                >
                  Remove from library, keep files
                </button>
              ) : (
                <>
                  <button
                    role="menuitem"
                    onClick={() => {
                      setMoreOpen(false);
                      onFavorite(false);
                    }}
                  >
                    Remove from favourites
                  </button>
                  <div className="menu-group" role="group" aria-label="Rate selection">
                    <span className="menu-group-label">Rate</span>
                    <div className="menu-stars">
                      {[1, 2, 3, 4, 5].map((n) => (
                        <button
                          key={n}
                          type="button"
                          className="menu-star"
                          title={`Rate ${n} star${n > 1 ? "s" : ""} (press ${n})`}
                          onClick={() => {
                            setMoreOpen(false);
                            onRate(n);
                          }}
                        >
                          {"★".repeat(n)}
                        </button>
                      ))}
                      <button
                        type="button"
                        className="menu-star clear"
                        title="Clear rating (press 0)"
                        onClick={() => {
                          setMoreOpen(false);
                          onRate(0);
                        }}
                      >
                        Clear
                      </button>
                    </div>
                  </div>
                  <div
                    className="menu-group"
                    role="group"
                    aria-label="Colour label"
                  >
                    <span className="menu-group-label">Label</span>
                    <div className="menu-swatches">
                      {LABEL_COLORS.map((color) => (
                        <button
                          key={color.id}
                          type="button"
                          className="swatch"
                          style={{ background: color.hex }}
                          title={`Label ${color.id}`}
                          aria-label={`Label ${color.id}`}
                          onClick={() => {
                            setMoreOpen(false);
                            onLabel(color.id);
                          }}
                        />
                      ))}
                      <button
                        type="button"
                        className="swatch none"
                        title="Remove colour label"
                        aria-label="Remove colour label"
                        onClick={() => {
                          setMoreOpen(false);
                          onLabel(null);
                        }}
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                </>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
