import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { Icon } from "../../components/icons";
import type { ExportRecord } from "../../lib/tauri";

/** Past ZIP exports with save locations and counts. */
export function ExportsView({
  exports,
  onRefresh,
  onOpenInFolder,
  onBrowseLibrary,
}: {
  exports: ExportRecord[];
  onRefresh: () => void;
  onOpenInFolder: (path: string) => void;
  onBrowseLibrary: () => void;
}) {
  return (
    <div className="exports-page">
      <PageHeader
        title="Exports"
        description="ZIP archives you've created from selected photos, with save location and item counts."
        actions={<button onClick={onRefresh}>Refresh</button>}
      />
      {exports.length === 0 ? (
        <EmptyState
          icon="download"
          title="No exports yet"
          description="Select photos in the library and choose Export ZIP from the selection bar. Finished archives will appear here."
          action={{ label: "Browse library", onClick: onBrowseLibrary }}
        />
      ) : (
        <ul className="export-cards">
          {exports.map((row) => {
            const name = row.path.split("/").pop() ?? row.path;
            return (
              <li key={row.id} className="export-card">
                <div className="export-card-icon" aria-hidden>
                  <Icon name="download" />
                </div>
                <div className="export-card-body">
                  <div className="export-card-title" title={row.path}>
                    {name}
                  </div>
                  <div className="export-card-path" title={row.path}>
                    {row.path}
                  </div>
                  <div className="export-card-meta">
                    <span>
                      {row.exportedCount} exported
                    </span>
                    {row.missingCount > 0 && (
                      <span className="export-warn">
                        {row.missingCount} missing
                      </span>
                    )}
                    <span>{row.assetCount} selected</span>
                    <span>
                      {new Date(row.createdAt).toLocaleString()}
                    </span>
                  </div>
                  {row.note && (
                    <div className="muted export-card-note">{row.note}</div>
                  )}
                </div>
                <button
                  type="button"
                  className="export-open-btn"
                  title="Show in Finder"
                  onClick={() => onOpenInFolder(row.path)}
                >
                  Show in folder
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
