import { useEffect, useState } from "react";
import { Icon, type IconName } from "../icons";
import {
  isSmartCollectionKind,
  type LibraryStats,
  type SmartCounts,
} from "../../lib/tauri";
import type { View } from "../../types/app";

type NavEntry = {
  id: View;
  label: string;
  icon: IconName;
  /** Planned destinations are shown for orientation but can't be opened yet. */
  soon?: never;
};

type SoonEntry = {
  id: string;
  label: string;
  icon: IconName;
  soon: true;
};

type NavSection = {
  heading: string;
  items: readonly (NavEntry | SoonEntry)[];
};

/**
 * Grouped navigation. Entries marked `soon` are placeholders for features that
 * need work not yet done (faces, GPS, OCR, a preferences store), so they render
 * disabled instead of leading to an empty screen.
 */
const NAV_SECTIONS: readonly NavSection[] = [
  {
    heading: "Library",
    items: [
      { id: "home", label: "Home", icon: "home" },
      { id: "library", label: "Library", icon: "library" },
      { id: "timeline", label: "Timeline", icon: "calendar" },
      { id: "favorites", label: "Favorites", icon: "star" },
      { id: "recent", label: "Recently added", icon: "history" },
      { id: "recentViewed", label: "Recently viewed", icon: "eye" },
    ],
  },
  {
    heading: "Discover",
    items: [
      { id: "people", label: "People", icon: "person", soon: true },
      { id: "places", label: "Places", icon: "place", soon: true },
      { id: "tags", label: "Tags", icon: "label" },
      { id: "albums", label: "Albums", icon: "album" },
      { id: "savedSearches", label: "Recent searches", icon: "search" },
    ],
  },
  {
    heading: "Smart collections",
    items: [
      { id: "screenshots", label: "Screenshots", icon: "sparkle" },
      { id: "selfies", label: "Selfies", icon: "selfie" },
      { id: "panoramas", label: "Panoramas", icon: "panorama" },
      { id: "videos", label: "Videos", icon: "play" },
      { id: "rawPhotos", label: "RAW photos", icon: "camera" },
      { id: "documents", label: "Documents", icon: "document", soon: true },
      { id: "receipts", label: "Receipts", icon: "receipt", soon: true },
    ],
  },
  {
    heading: "Private",
    items: [{ id: "locked", label: "Locked folder", icon: "lock" }],
  },
  {
    heading: "Manage",
    items: [
      { id: "watched", label: "Watched folders", icon: "folder" },
      { id: "trash", label: "Trash", icon: "trash" },
      { id: "exports", label: "Exports", icon: "download" },
      { id: "duplicates", label: "Duplicates", icon: "copy" },
      { id: "activity", label: "Activity", icon: "activity" },
    ],
  },
  {
    heading: "Settings",
    items: [
      { id: "settings", label: "Settings", icon: "settings" },
      { id: "developer", label: "Developer", icon: "code" },
    ],
  },
];

function sectionForView(view: View): string | null {
  for (const section of NAV_SECTIONS) {
    if (section.items.some((item) => !item.soon && item.id === view)) {
      return section.heading;
    }
  }
  return null;
}

function defaultExpanded(): Record<string, boolean> {
  return Object.fromEntries(NAV_SECTIONS.map((s) => [s.heading, true]));
}

/** App navigation rail with import action and library stats footer. */
export function Sidebar({
  view,
  stats,
  smartCounts,
  albumCount,
  tagCount,
  savedSearchCount,
  exportCount,
  lockedCount,
  busy,
  onImport,
  onNavigate,
}: {
  view: View;
  stats: LibraryStats | null;
  smartCounts: SmartCounts | null;
  albumCount: number;
  tagCount: number;
  savedSearchCount: number;
  exportCount: number;
  lockedCount: number;
  busy: boolean;
  onImport: () => void;
  onNavigate: (view: View) => void;
}) {
  const [expanded, setExpanded] = useState(defaultExpanded);

  // Keep the section that owns the active view open so the selection stays visible.
  useEffect(() => {
    const heading = sectionForView(view);
    if (!heading) return;
    setExpanded((prev) => (prev[heading] ? prev : { ...prev, [heading]: true }));
  }, [view]);

  function toggleSection(heading: string) {
    setExpanded((prev) => ({ ...prev, [heading]: !prev[heading] }));
  }

  function countFor(id: View): number | null {
    switch (id) {
      case "trash":
        return stats?.inTrash ?? null;
      case "favorites":
        return stats?.favorites ?? null;
      case "albums":
        return albumCount;
      case "tags":
        return tagCount;
      case "savedSearches":
        return savedSearchCount;
      case "locked":
        return lockedCount;
      case "exports":
        return exportCount;
      default:
        return isSmartCollectionKind(id)
          ? (smartCounts?.[id] ?? null)
          : null;
    }
  }

  return (
    <aside className="sidebar">
      <div className="sidebar-head">
        <h1 className="brand">LUMORA</h1>
        <p className="muted">your memories your machine.</p>
        <button className="primary sidebar-import" onClick={onImport} disabled={busy}>
          {busy ? "Importing…" : "+ Import photos"}
        </button>
      </div>

      <nav className="sidebar-nav">
        {NAV_SECTIONS.map((section) => {
          const isOpen = expanded[section.heading] !== false;
          return (
            <div
              key={section.heading}
              className={`nav-section ${isOpen ? "open" : "collapsed"}`}
            >
              <button
                type="button"
                className="nav-section-toggle"
                aria-expanded={isOpen}
                onClick={() => toggleSection(section.heading)}
              >
                <Icon name="chevronRight" className="nav-section-chevron" />
                <span>{section.heading}</span>
              </button>
              {isOpen && (
                <div className="nav-section-items">
                  {section.items.map((item) => {
                    if (item.soon) {
                      return (
                        <button
                          key={item.id}
                          className="nav-btn nav-btn-soon"
                          disabled
                          title={`${item.label} — coming soon`}
                        >
                          <Icon name={item.icon} className="nav-icon" />
                          <span>{item.label}</span>
                          <span className="nav-soon-pill">Soon</span>
                        </button>
                      );
                    }
                    const count = countFor(item.id);
                    return (
                      <button
                        key={item.id}
                        className={`nav-btn ${view === item.id ? "active" : ""}`}
                        onClick={() => onNavigate(item.id)}
                      >
                        <Icon name={item.icon} className="nav-icon" />
                        <span>{item.label}</span>
                        {count ? <span className="nav-count">{count}</span> : null}
                      </button>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </nav>

      {stats && (
        <div className="sidebar-footer">
          <span>
            <strong>{stats.totalAssets}</strong> assets
          </span>
          <span>
            <strong>{stats.totalImages}</strong> photos
          </span>
          <span>
            <strong>{stats.totalVideos}</strong> videos
          </span>
        </div>
      )}
    </aside>
  );
}
