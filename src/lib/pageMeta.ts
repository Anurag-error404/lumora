import type { View } from "../types/app";

/** Titles and blurbs for library-style grids that don't own a dedicated page shell. */
export const LIBRARY_PAGE_META: Partial<
  Record<View, { title: string; description: string }>
> = {
  library: {
    title: "Library",
    description:
      "Everything in your local collection — photos and videos, newest capture first.",
  },
  recent: {
    title: "Recently added",
    description:
      "Fresh imports and newly indexed files, so you can review what just arrived.",
  },
  recentViewed: {
    title: "Recently viewed",
    description:
      "Pick up where you left off. Opening a photo in the viewer adds it here.",
  },
  favorites: {
    title: "Favorites",
    description:
      "Shots you've marked with a heart — your personal highlights, all in one place.",
  },
  trash: {
    title: "Trash",
    description:
      "Soft-deleted library entries. Restore them, or empty the trash when you're sure.",
  },
  videos: {
    title: "Videos",
    description:
      "A smart collection of every video in the library. No manual sorting required.",
  },
  rawPhotos: {
    title: "RAW photos",
    description:
      "Camera RAW files (DNG, CR2, NEF, ARW, and more) gathered automatically by extension.",
  },
  screenshots: {
    title: "Screenshots",
    description:
      "Screen captures recognised by filename or folder — and without camera EXIF.",
  },
  selfies: {
    title: "Selfies",
    description:
      "Front-camera shots, picked out from the lens recorded in each photo's EXIF.",
  },
  panoramas: {
    title: "Panoramas",
    description:
      "Wide and tall stitched shots — anything at least twice as long as it is deep.",
  },
  documents: {
    title: "Documents",
    description:
      "Photos with substantial on-device OCR text — scans, notes, and pages that read like documents.",
  },
  receipts: {
    title: "Receipts",
    description:
      "Photos whose extracted text looks like a receipt or invoice (totals, tax, currency tokens).",
  },
  people: {
    title: "People",
    description:
      "Faces grouped on-device. Name a person once to browse every photo they appear in.",
  },
};
