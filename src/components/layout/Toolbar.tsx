import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Icon, IconButton } from "../icons";
import type { SavedSearch } from "../../lib/tauri";

/** Search field with recent-query hints, plus undo/redo and trash empty. */
export function Toolbar({
  query,
  onQueryChange,
  onSubmitSearch,
  recentSearches,
  onPickRecent,
  canUndo,
  canRedo,
  onUndo,
  onRedo,
  isTrashView,
  emptyTrashDisabled,
  onEmptyTrash,
}: {
  query: string;
  onQueryChange: (query: string) => void;
  onSubmitSearch: () => void;
  recentSearches: SavedSearch[];
  onPickRecent: (query: string) => void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
  isTrashView: boolean;
  emptyTrashDisabled: boolean;
  onEmptyTrash: () => void;
}) {
  const listId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);

  const hints = useMemo(() => {
    const q = query.trim().toLowerCase();
    const rows = recentSearches.map((s) => s.query);
    if (!q) return rows.slice(0, 8);
    return rows
      .filter((text) => text.toLowerCase().includes(q) && text.toLowerCase() !== q)
      .slice(0, 8);
  }, [query, recentSearches]);

  useEffect(() => {
    setHighlight(0);
  }, [hints]);

  useEffect(() => {
    function onPointerDown(e: PointerEvent) {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    }
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, []);

  function pick(text: string) {
    setOpen(false);
    onPickRecent(text);
  }

  return (
    <div className="toolbar">
      <div className="search-field" ref={rootRef}>
        <Icon name="search" className="search-icon" />
        <input
          type="search"
          role="combobox"
          aria-expanded={open && hints.length > 0}
          aria-controls={listId}
          aria-autocomplete="list"
          placeholder="Search — dog on a beach, or camera:iphone rating>3"
          value={query}
          onChange={(e) => {
            onQueryChange(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown" && hints.length) {
              e.preventDefault();
              setOpen(true);
              setHighlight((i) => (i + 1) % hints.length);
              return;
            }
            if (e.key === "ArrowUp" && hints.length) {
              e.preventDefault();
              setOpen(true);
              setHighlight((i) => (i - 1 + hints.length) % hints.length);
              return;
            }
            if (e.key === "Escape") {
              setOpen(false);
              return;
            }
            if (e.key === "Enter") {
              if (open && hints[highlight]) {
                e.preventDefault();
                pick(hints[highlight]);
                return;
              }
              setOpen(false);
              onSubmitSearch();
            }
          }}
        />
        {open && hints.length > 0 && (
          <ul
            id={listId}
            className="search-hints"
            role="listbox"
            aria-label="Recent searches"
          >
            <li className="search-hints-label" role="presentation">
              Recent
            </li>
            {hints.map((text, index) => (
              <li key={text} role="option" aria-selected={index === highlight}>
                <button
                  type="button"
                  className={index === highlight ? "is-active" : ""}
                  onMouseEnter={() => setHighlight(index)}
                  onMouseDown={(e) => {
                    // Keep focus behavior predictable before blur closes the list.
                    e.preventDefault();
                    pick(text);
                  }}
                >
                  <Icon name="history" className="search-hint-icon" />
                  <span>{text}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="spacer" />
      <IconButton
        icon="undo"
        label="Undo last action"
        onClick={onUndo}
        disabled={!canUndo}
      />
      <IconButton
        icon="redo"
        label="Redo last action"
        onClick={onRedo}
        disabled={!canRedo}
      />
      {isTrashView && (
        <button
          className="danger"
          onClick={onEmptyTrash}
          disabled={emptyTrashDisabled}
        >
          Empty trash…
        </button>
      )}
    </div>
  );
}
