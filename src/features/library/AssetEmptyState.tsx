import { EmptyState } from "../../components/EmptyState";
import type { Album } from "../../lib/tauri";
import type { View } from "../../types/app";

/** Context-aware empty state for the asset grid views. */
export function AssetEmptyState({
  view,
  albums,
  activeAlbum,
  tagBrowseActive,
  onClearTagBrowse,
  query,
  onClearSearch,
  isPicking,
  onImport,
  onBrowseLibrary,
  onStartPicking,
}: {
  view: View;
  albums: Album[];
  activeAlbum: string | null;
  tagBrowseActive: boolean;
  onClearTagBrowse: () => void;
  query: string;
  onClearSearch: () => void;
  isPicking: boolean;
  onImport: () => void;
  onBrowseLibrary: () => void;
  onStartPicking: (album: { id: string; name: string }) => void;
}) {
  if (view === "albums" && activeAlbum) {
    const album = albums.find((item) => item.id === activeAlbum);
    return (
      <EmptyState
        icon="album"
        title="This album is ready for its first photo"
        description="Add photos from your library to turn this empty album into a collection."
        action={
          album
            ? {
                label: "Add photos",
                onClick: () => onStartPicking(album),
              }
            : undefined
        }
      />
    );
  }
  if (view === "tags") {
    return tagBrowseActive ? (
      <EmptyState
        icon="search"
        title="No photos match this combination"
        description="The selected conditions are too narrow. Remove one or more filters to reveal matching photos."
        action={{ label: "Clear filters", onClick: onClearTagBrowse }}
      />
    ) : (
      <EmptyState
        icon="label"
        title="Build a photo filter"
        description="Choose a tag, rating, or colour above. Combine them to narrow the library precisely."
      />
    );
  }
  if (query.trim()) {
    return (
      <EmptyState
        icon="search"
        title="No matching photos"
        description={`Nothing in your library matches “${query.trim()}”. Try a broader search or clear it to see everything.`}
        action={{ label: "Clear search", onClick: onClearSearch }}
      />
    );
  }
  if (isPicking) {
    return (
      <EmptyState
        icon="library"
        title="No photos available to add"
        description="Import photos into your library first, then return to fill this album."
        action={{
          label: "Import photos",
          onClick: onImport,
        }}
      />
    );
  }
  if (view === "favorites") {
    return (
      <EmptyState
        icon="heartOutline"
        title="No favourites yet"
        description="Tap the heart on any photo to keep your best moments together here."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "recent") {
    return (
      <EmptyState
        icon="history"
        title="Nothing added recently"
        description="New imports will appear here so you can quickly review your latest photos."
        action={{
          label: "Import photos",
          onClick: onImport,
        }}
      />
    );
  }
  if (view === "recentViewed") {
    return (
      <EmptyState
        icon="eye"
        title="Nothing viewed yet"
        description="Open a photo or video in the media viewer and it will show up here."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "videos") {
    return (
      <EmptyState
        icon="play"
        title="No videos yet"
        description="Videos you import land here automatically, alongside your photos."
        action={{ label: "Import media", onClick: onImport }}
      />
    );
  }
  if (view === "rawPhotos") {
    return (
      <EmptyState
        icon="camera"
        title="No RAW photos yet"
        description="Files such as DNG, CR2, NEF, and ARW are collected here as soon as they're imported."
        action={{ label: "Import media", onClick: onImport }}
      />
    );
  }
  if (view === "screenshots") {
    return (
      <EmptyState
        icon="sparkle"
        title="No screenshots found"
        description="Screen captures are recognised by their filename or folder and grouped here automatically."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "selfies") {
    return (
      <EmptyState
        icon="selfie"
        title="No selfies found"
        description="Front-camera shots are recognised from the lens recorded in each photo's EXIF."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "panoramas") {
    return (
      <EmptyState
        icon="panorama"
        title="No panoramas found"
        description="Stitched shots at least twice as wide as they are tall — or twice as tall as they are wide — collect here."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "documents") {
    return (
      <EmptyState
        icon="document"
        title="No documents yet"
        description="Install OCR models in Settings → AI Features, then photos with substantial extracted text appear here."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "receipts") {
    return (
      <EmptyState
        icon="receipt"
        title="No receipts yet"
        description="After OCR runs, photos whose text looks like a receipt or invoice (totals, tax, currency) collect here."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "people") {
    return (
      <EmptyState
        icon="person"
        title="No photos for this person"
        description="Open another person from Discover → People, or wait for face detection to finish in Settings."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  if (view === "trash") {
    return (
      <EmptyState
        icon="trash"
        title="Trash is empty"
        description="Deleted library entries appear here temporarily before the retention period ends."
        action={{ label: "Browse library", onClick: onBrowseLibrary }}
      />
    );
  }
  return (
    <EmptyState
      icon="camera"
      title="Your photo library is waiting"
      description="Import photos, videos, or a folder to begin organising your collection."
      action={{
        label: "Import media",
        onClick: onImport,
      }}
    />
  );
}
