import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { Icon } from "../../components/icons";
import { MediaFallback } from "../../components/MediaFallback";
import type { AssetSummary, DuplicateGroup } from "../../lib/tauri";
import { AssetThumb } from "../library/AssetThumb";

/** Duplicate groups with per-group and bulk cleanup actions. */
export function DuplicatesView({
  dupes,
  dupeAssets,
  onRefresh,
  onCleanupAll,
  onCleanupGroup,
  onPreview,
  onShowInfo,
  onBrowseLibrary,
}: {
  dupes: DuplicateGroup[];
  dupeAssets: Map<string, AssetSummary>;
  onRefresh: () => void;
  onCleanupAll: () => void;
  onCleanupGroup: (group: DuplicateGroup, keepId: string) => void;
  onPreview: (id: string) => void;
  onShowInfo: (id: string) => void;
  onBrowseLibrary: () => void;
}) {
  return (
    <div className="panel-list">
      <PageHeader
        title="Duplicates"
        description="Exact matches by file hash, plus near-duplicates spotted by perceptual similarity. Keep one, trash the rest."
        actions={
          <>
            <button onClick={onRefresh}>Refresh</button>
            {dupes.length > 0 && (
              <button className="danger" onClick={onCleanupAll}>
                Clean up all — keep 1 per group
              </button>
            )}
          </>
        }
      />
      {dupes.length > 0 && (
        <p className="muted dupe-count-line">
          {dupes.length} duplicate group{dupes.length === 1 ? "" : "s"} found
        </p>
      )}
      {dupes.length === 0 ? (
        <EmptyState
          icon="copy"
          title="No duplicates found"
          description="Your library looks tidy. Refresh after importing more photos to scan again."
          action={{ label: "Scan again", onClick: onRefresh }}
          secondaryAction={{
            label: "Browse library",
            onClick: onBrowseLibrary,
          }}
        />
      ) : dupes.map((g) => (
        <div key={`${g.kind}-${g.key}`} className="dupe-group">
          <div className="dupe-group-header">
            <span className="muted">
              {g.kind === "exact"
                ? "Exact duplicates"
                : "Near duplicates (Hamming ≤ 5)"}
              {" · "}
              {g.assetIds.length} items
            </span>
            <button onClick={() => onCleanupGroup(g, g.assetIds[0])}>
              Keep first, trash {g.assetIds.length - 1}
            </button>
          </div>
          <div className="asset-grid dupe-grid">
            {g.assetIds.map((id) => {
              const asset = dupeAssets.get(id);
              const name =
                asset?.path.split("/").pop() ?? id.slice(0, 8);
              return (
                <div key={id} className="asset-card dupe-card">
                  <div
                    className="dupe-thumb"
                    title="Click to preview"
                    onClick={() => onPreview(id)}
                  >
                    {asset ? (
                      <AssetThumb asset={asset} />
                    ) : (
                      <MediaFallback type="image" />
                    )}
                    {asset && (
                      <button
                        type="button"
                        className="info-btn dupe-info-btn"
                        title="Media information"
                        aria-label="Show media information"
                        onClick={(e) => {
                          e.stopPropagation();
                          onShowInfo(asset.id);
                        }}
                      >
                        <Icon name="info" />
                      </button>
                    )}
                  </div>
                  <div className="dupe-card-footer">
                    <span className="dupe-name" title={asset?.path}>
                      {name}
                    </span>
                    <button
                      className="dupe-keep"
                      title="Keep this photo and move the rest of the group to trash"
                      onClick={(e) => {
                        e.stopPropagation();
                        onCleanupGroup(g, id);
                      }}
                    >
                      Keep this
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
