import type { TimelineMonth } from "../lib/tauri";

export type View =
  | "home"
  | "library"
  | "recent"
  | "recentViewed"
  | "timeline"
  | "albums"
  | "tags"
  | "savedSearches"
  | "duplicates"
  | "videos"
  | "rawPhotos"
  | "screenshots"
  | "selfies"
  | "panoramas"
  | "trash"
  | "favorites"
  | "locked"
  | "watched"
  | "activity"
  | "exports"
  | "settings"
  | "developer";

export type AlbumModalMode = "create" | "move" | null;

export type AlbumPickTarget = { id: string; name: string };

export type Marquee = { x: number; y: number; w: number; h: number };

export type TimelineYearGroup = {
  year: number;
  months: TimelineMonth[];
};
