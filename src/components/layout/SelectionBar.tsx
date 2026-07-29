import { useEffect, useRef, useState } from "react";
import { IconButton } from "../icons";
import { LABEL_COLORS } from "../../lib/constants";
import type { PluginEntry } from "../../lib/tauri";

/**
 * Action bar shown while photos are selected: favourites, tagging, export,
 * albums, trash, and a "more" menu with rating and colour labels.
 */
export function SelectionBar({
  selectedCount,
  isTrashView,
  busy,
  pluginBusy = false,
  plugins,
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
  onRunPlugin,
}: {
  selectedCount: number;
  isTrashView: boolean;
  busy: boolean;
  pluginBusy?: boolean;
  plugins?: PluginEntry[];
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
  onRunPlugin?: (pluginId: string, actionId: string) => void;
}) {
  const [moreOpen, setMoreOpen] = useState(false);
  const [pluginOpen, setPluginOpen] = useState(false);
  const pluginAnchorRef = useRef<HTMLDivElement>(null);

  // Close plugin menu when clicking outside
  useEffect(() => {
    if (!pluginOpen) return;
    const handler = (e: MouseEvent) => {
      if (!pluginAnchorRef.current?.contains(e.target as Node)) {
        setPluginOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [pluginOpen]);

  const enabledPlugins = (plugins ?? []).filter(
    (p) => p.enabled && p.manifest.contributions.actions.length > 0,
  );

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

          {/* Plugin actions button */}
          {onRunPlugin && (
            <div className="menu-anchor" ref={pluginAnchorRef}>
              <button
                type="button"
                className={`selection-plugin-btn${enabledPlugins.length === 0 ? " is-empty" : ""}`}
                title={
                  enabledPlugins.length === 0
                    ? "No plugins enabled — go to Plugins to install some"
                    : pluginBusy
                      ? "Plugin is running…"
                      : "Run a plugin on selected photos"
                }
                aria-haspopup="true"
                aria-expanded={pluginOpen}
                disabled={enabledPlugins.length === 0 || pluginBusy}
                onClick={() => setPluginOpen((v) => (pluginBusy ? false : !v))}
              >
                {/* Puzzle icon */}
                <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
                  <path d="M20.5 11H19V7a2 2 0 0 0-2-2h-4V3.5a2.5 2.5 0 0 0-5 0V5H4a2 2 0 0 0-2 2v3.8h1.5c1.5 0 2.7 1.2 2.7 2.7S5 16.2 3.5 16.2H2V20a2 2 0 0 0 2 2h3.8v-1.5a2.7 2.7 0 0 1 5.4 0V22H17a2 2 0 0 0 2-2v-4h1.5a2.5 2.5 0 0 0 0-5z" />
                </svg>
                <span className="selection-plugin-label">
                  {pluginBusy ? "Running…" : "Plugins"}
                </span>
                {enabledPlugins.length > 0 && (
                  <span className="selection-plugin-count">{enabledPlugins.length}</span>
                )}
              </button>

              {pluginOpen && enabledPlugins.length > 0 && (
                <>
                  <div className="menu-backdrop" onClick={() => setPluginOpen(false)} />
                  <div className="menu plugin-action-menu" role="menu" aria-label="Plugin actions">
                    <div className="plugin-action-menu-header">
                      <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor" aria-hidden="true">
                        <path d="M20.5 11H19V7a2 2 0 0 0-2-2h-4V3.5a2.5 2.5 0 0 0-5 0V5H4a2 2 0 0 0-2 2v3.8h1.5c1.5 0 2.7 1.2 2.7 2.7S5 16.2 3.5 16.2H2V20a2 2 0 0 0 2 2h3.8v-1.5a2.7 2.7 0 0 1 5.4 0V22H17a2 2 0 0 0 2-2v-4h1.5a2.5 2.5 0 0 0 0-5z" />
                      </svg>
                      Run plugin on {selectedCount} photo{selectedCount !== 1 ? "s" : ""}
                    </div>
                    {enabledPlugins.map((plugin) =>
                      plugin.manifest.contributions.actions.map((action) => (
                        <button
                          key={`${plugin.manifest.id}::${action.id}`}
                          role="menuitem"
                          className="plugin-action-menuitem"
                          disabled={pluginBusy}
                          onClick={() => {
                            setPluginOpen(false);
                            onRunPlugin(plugin.manifest.id, action.id);
                          }}
                        >
                          <span className="plugin-action-menuitem-name">
                            {action.label}
                          </span>
                          <span className="plugin-action-menuitem-plugin muted">
                            {plugin.manifest.name}
                          </span>
                        </button>
                      )),
                    )}
                  </div>
                </>
              )}
            </div>
          )}
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
