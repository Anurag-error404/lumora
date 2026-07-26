import type { IndexProgress } from "../../lib/tauri";

/** Bottom status line: counts, indexer state, and interaction hints. */
export function StatusBar({
  selectedCount,
  isTimelineView,
  visibleTimelineAssetCount,
  assetCount,
  progress,
  isPicking,
}: {
  selectedCount: number;
  isTimelineView: boolean;
  visibleTimelineAssetCount: number;
  assetCount: number;
  progress: IndexProgress | null;
  isPicking: boolean;
}) {
  return (
    <div className="status-bar">
      {selectedCount > 0
        ? `${selectedCount} selected`
        : isTimelineView
          ? `${visibleTimelineAssetCount} shown`
          : `${assetCount} shown`}
      {progress?.running || (progress && progress.pending > 0)
        ? ` · indexing ${progress.pending} pending · ${progress.processed} processed`
        : ""}
      {isPicking
        ? " · click to select · drag to marquee · ⌘A select all"
        : " · click to preview · ⌘ click or checkbox to select · drag to marquee · ⌥ click for single · ←/→ to browse · 1-5 rate · Del trash"}
    </div>
  );
}
