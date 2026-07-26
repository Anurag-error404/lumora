import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { Icon } from "../../components/icons";

/** List / add / remove watched library folders. */
export function WatchedFoldersView({
  folders,
  loading,
  busy,
  onRefresh,
  onAdd,
  onRemove,
}: {
  folders: string[];
  loading: boolean;
  busy: boolean;
  onRefresh: () => void;
  onAdd: () => void;
  onRemove: (path: string) => void;
}) {
  return (
    <div className="watched-page">
      <PageHeader
        title="Watched folders"
        description="Folders LUMORA keeps in sync. New, changed, or removed media update the library automatically."
        actions={
          <>
            <button onClick={onRefresh} disabled={loading || busy}>
              {loading ? "Refreshing…" : "Refresh"}
            </button>
            <button className="primary" onClick={onAdd} disabled={busy}>
              Add folder…
            </button>
          </>
        }
      />

      {loading && folders.length === 0 ? (
        <div className="developer-loading" role="status">
          <span className="spinner" aria-hidden="true" />
          Loading watched folders…
        </div>
      ) : folders.length === 0 ? (
        <EmptyState
          icon="folder"
          title="No watched folders yet"
          description="Add a folder to keep the library in sync as files appear or change on disk."
          action={{ label: "Add folder", onClick: onAdd }}
        />
      ) : (
        <ul className="watched-list">
          {folders.map((path) => {
            const name = path.split(/[/\\]/).filter(Boolean).pop() ?? path;
            return (
              <li key={path} className="watched-card">
                <div className="watched-card-icon" aria-hidden>
                  <Icon name="folder" />
                </div>
                <div className="watched-card-body">
                  <div className="watched-card-title" title={path}>
                    {name}
                  </div>
                  <div className="watched-card-path" title={path}>
                    {path}
                  </div>
                </div>
                <button
                  type="button"
                  className="watched-remove-btn"
                  disabled={busy}
                  title="Stop watching"
                  onClick={() => onRemove(path)}
                >
                  Remove
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
