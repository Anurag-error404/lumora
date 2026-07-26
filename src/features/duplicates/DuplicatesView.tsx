import { useState, type ReactNode } from "react";
import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { Icon } from "../../components/icons";
import { MediaFallback } from "../../components/MediaFallback";
import type {
  AssetSummary,
  BlurryAsset,
  DuplicateGroup,
} from "../../lib/tauri";
import { AssetThumb } from "../library/AssetThumb";

/** Duplicate groups with per-kind cleanup, plus blurry-image review. */
export function DuplicatesView({
  dupes,
  dupeAssets,
  blurry,
  onRefresh,
  onCleanupExact,
  onCleanupGroup,
  onTrashBlurry,
  onPreview,
  onShowInfo,
  onBrowseLibrary,
}: {
  dupes: DuplicateGroup[];
  dupeAssets: Map<string, AssetSummary>;
  blurry: BlurryAsset[];
  onRefresh: () => void;
  onCleanupExact: () => void;
  onCleanupGroup: (group: DuplicateGroup, keepId: string) => void;
  onTrashBlurry: (ids: string[]) => void;
  onPreview: (id: string) => void;
  onShowInfo: (id: string) => void;
  onBrowseLibrary: () => void;
}) {
  const exactGroups = dupes.filter((g) => g.kind === "exact");
  const nearGroups = dupes.filter((g) => g.kind !== "exact");
  const exactItems = exactGroups.reduce((n, g) => n + g.assetIds.length, 0);
  const nearItems = nearGroups.reduce((n, g) => n + g.assetIds.length, 0);
  const hasAnything =
    exactGroups.length > 0 || nearGroups.length > 0 || blurry.length > 0;

  return (
    <div className="panel-list">
      <PageHeader
        title="Duplicates"
        description="Exact matches share the same file hash. Near duplicates look similar. Blurry photos are flagged for optional cleanup."
        actions={<button onClick={onRefresh}>Refresh</button>}
      />

      {hasAnything && (
        <div className="dupe-summary" aria-label="Cleanup summary">
          <span className="dupe-summary-chip exact">
            <Icon name="copy" />
            <strong>{exactGroups.length}</strong>
            exact group{exactGroups.length === 1 ? "" : "s"}
            <em>{exactItems} files</em>
          </span>
          <span className="dupe-summary-chip near">
            <Icon name="sparkle" />
            <strong>{nearGroups.length}</strong>
            near group{nearGroups.length === 1 ? "" : "s"}
            <em>{nearItems} files</em>
          </span>
          <span className="dupe-summary-chip blurry">
            <Icon name="eye" />
            <strong>{blurry.length}</strong>
            blurry
          </span>
        </div>
      )}

      {!hasAnything ? (
        <EmptyState
          icon="copy"
          title="Nothing to clean up"
          description="No exact duplicates, near duplicates, or blurry images found. Refresh after importing more photos."
          action={{ label: "Scan again", onClick: onRefresh }}
          secondaryAction={{
            label: "Browse library",
            onClick: onBrowseLibrary,
          }}
        />
      ) : (
        <>
          <DupeSection
            kind="exact"
            title="Exact duplicates"
            blurb="Identical file contents (same SHA-256). Safe to keep one and trash the rest."
            groups={exactGroups}
            dupeAssets={dupeAssets}
            onCleanupGroup={onCleanupGroup}
            onPreview={onPreview}
            onShowInfo={onShowInfo}
            defaultOpen={exactGroups.length > 0}
            sectionAction={
              exactGroups.length > 0 ? (
                <button
                  className="danger"
                  onClick={(e) => {
                    e.stopPropagation();
                    onCleanupExact();
                  }}
                >
                  Clean up all exact — keep 1 per group
                </button>
              ) : undefined
            }
          />
          <DupeSection
            kind="near"
            title="Near duplicates"
            blurb="Visually similar by perceptual hash (Hamming ≤ 2). Review each group — no bulk delete."
            groups={nearGroups}
            dupeAssets={dupeAssets}
            onCleanupGroup={onCleanupGroup}
            onPreview={onPreview}
            onShowInfo={onShowInfo}
            defaultOpen={nearGroups.length > 0}
          />
          <BlurrySection
            blurry={blurry}
            dupeAssets={dupeAssets}
            onTrashBlurry={onTrashBlurry}
            onPreview={onPreview}
            onShowInfo={onShowInfo}
            defaultOpen={blurry.length > 0}
          />
        </>
      )}
    </div>
  );
}

function DupeSection({
  kind,
  title,
  blurb,
  groups,
  dupeAssets,
  onCleanupGroup,
  onPreview,
  onShowInfo,
  sectionAction,
  defaultOpen,
}: {
  kind: "exact" | "near";
  title: string;
  blurb: string;
  groups: DuplicateGroup[];
  dupeAssets: Map<string, AssetSummary>;
  onCleanupGroup: (group: DuplicateGroup, keepId: string) => void;
  onPreview: (id: string) => void;
  onShowInfo: (id: string) => void;
  sectionAction?: ReactNode;
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const empty = groups.length === 0;

  return (
    <section
      className={`dupe-section dupe-section-${kind}${empty ? " is-empty" : ""}${open ? " is-open" : " is-collapsed"}`}
    >
      <header className="dupe-section-header">
        <button
          type="button"
          className="dupe-collapse-toggle"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          <Icon name="chevronRight" className="dupe-collapse-chevron" />
          <span className={`dupe-kind-badge ${kind}`}>
            <Icon name={kind === "exact" ? "copy" : "sparkle"} />
            {kind === "exact" ? "Exact" : "Near"}
          </span>
          <div className="dupe-collapse-copy">
            <h2>{title}</h2>
            <p className="muted">{blurb}</p>
          </div>
        </button>
        <div className="dupe-section-actions">
          {sectionAction}
          <span className="dupe-section-count">
            {empty
              ? "None found"
              : `${groups.length} group${groups.length === 1 ? "" : "s"}`}
          </span>
        </div>
      </header>

      {open && groups.length > 0 && (
        <div className="dupe-section-groups">
          {groups.map((g) => (
            <DupeGroupCard
              key={`${g.kind}-${g.key}`}
              group={g}
              dupeAssets={dupeAssets}
              onCleanupGroup={onCleanupGroup}
              onPreview={onPreview}
              onShowInfo={onShowInfo}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function BlurrySection({
  blurry,
  dupeAssets,
  onTrashBlurry,
  onPreview,
  onShowInfo,
  defaultOpen,
}: {
  blurry: BlurryAsset[];
  dupeAssets: Map<string, AssetSummary>;
  onTrashBlurry: (ids: string[]) => void;
  onPreview: (id: string) => void;
  onShowInfo: (id: string) => void;
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const empty = blurry.length === 0;

  return (
    <section
      className={`dupe-section dupe-section-blurry${empty ? " is-empty" : ""}${open ? " is-open" : " is-collapsed"}`}
    >
      <header className="dupe-section-header">
        <button
          type="button"
          className="dupe-collapse-toggle"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          <Icon name="chevronRight" className="dupe-collapse-chevron" />
          <span className="dupe-kind-badge blurry">
            <Icon name="eye" />
            Blurry
          </span>
          <div className="dupe-collapse-copy">
            <h2>Blurry images</h2>
            <p className="muted">
              Soft or out-of-focus photos (Laplacian variance ≤ 80). Review
              before deleting — intentional soft focus can score low too.
            </p>
          </div>
        </button>
        <div className="dupe-section-actions">
          {blurry.length > 0 && (
            <button
              className="danger"
              onClick={(e) => {
                e.stopPropagation();
                onTrashBlurry(blurry.map((b) => b.asset.id));
              }}
            >
              Trash all listed ({blurry.length})
            </button>
          )}
          <span className="dupe-section-count">
            {empty
              ? "None found"
              : `${blurry.length} image${blurry.length === 1 ? "" : "s"}`}
          </span>
        </div>
      </header>

      {open && blurry.length > 0 && (
        <div className="asset-grid dupe-grid blurry-grid">
          {blurry.map((hit) => {
            const asset = dupeAssets.get(hit.asset.id) ?? hit.asset;
            const name = asset.path.split("/").pop() ?? asset.id.slice(0, 8);
            return (
              <div key={asset.id} className="asset-card dupe-card blurry-card">
                <div
                  className="dupe-thumb"
                  title="Click to preview"
                  onClick={() => onPreview(asset.id)}
                >
                  <AssetThumb asset={asset} />
                  <span
                    className="blurry-score"
                    title="Blur score (lower = softer)"
                  >
                    {hit.blurScore.toFixed(0)}
                  </span>
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
                </div>
                <div className="dupe-card-footer">
                  <span className="dupe-name" title={asset.path}>
                    {name}
                  </span>
                  <button
                    className="dupe-keep danger-text"
                    title="Move this blurry photo to trash"
                    onClick={(e) => {
                      e.stopPropagation();
                      onTrashBlurry([asset.id]);
                    }}
                  >
                    Trash
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

function DupeGroupCard({
  group,
  dupeAssets,
  onCleanupGroup,
  onPreview,
  onShowInfo,
}: {
  group: DuplicateGroup;
  dupeAssets: Map<string, AssetSummary>;
  onCleanupGroup: (group: DuplicateGroup, keepId: string) => void;
  onPreview: (id: string) => void;
  onShowInfo: (id: string) => void;
}) {
  const exact = group.kind === "exact";
  // Near groups start collapsed so many review rows stay scannable.
  const [open, setOpen] = useState(exact);

  const previewIds = group.assetIds.slice(0, 4);

  return (
    <div
      className={`dupe-group dupe-group-${exact ? "exact" : "near"}${open ? " is-open" : " is-collapsed"}`}
    >
      <div className="dupe-group-header">
        <button
          type="button"
          className="dupe-collapse-toggle dupe-group-toggle"
          aria-expanded={open}
          onClick={() => setOpen((v) => !v)}
        >
          <Icon name="chevronRight" className="dupe-collapse-chevron" />
          <span
            className={`dupe-kind-badge compact ${exact ? "exact" : "near"}`}
          >
            <Icon name={exact ? "copy" : "sparkle"} />
            {exact ? "Identical file" : "Similar look"}
          </span>
          <span className="muted">
            {group.assetIds.length} items
            {!exact && " · Hamming ≤ 2"}
          </span>
          {!open && (
            <span className="dupe-group-previews" aria-hidden>
              {previewIds.map((id) => {
                const asset = dupeAssets.get(id);
                return (
                  <span key={id} className="dupe-group-preview">
                    {asset ? (
                      <AssetThumb asset={asset} />
                    ) : (
                      <MediaFallback type="image" />
                    )}
                  </span>
                );
              })}
              {group.assetIds.length > previewIds.length && (
                <span className="dupe-group-preview-more">
                  +{group.assetIds.length - previewIds.length}
                </span>
              )}
            </span>
          )}
        </button>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onCleanupGroup(group, group.assetIds[0]);
          }}
        >
          Keep first, trash {group.assetIds.length - 1}
        </button>
      </div>
      {open && (
        <div className="asset-grid dupe-grid">
          {group.assetIds.map((id) => {
            const asset = dupeAssets.get(id);
            const name = asset?.path.split("/").pop() ?? id.slice(0, 8);
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
                      onCleanupGroup(group, id);
                    }}
                  >
                    Keep this
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
