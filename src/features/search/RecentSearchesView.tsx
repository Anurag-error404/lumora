import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import type { SavedSearch } from "../../lib/tauri";

/** Recent search history — recorded automatically when you run a search. */
export function RecentSearchesView({
  searches,
  onRun,
  onDelete,
  onClear,
}: {
  searches: SavedSearch[];
  onRun: (search: SavedSearch) => void;
  onDelete: (search: SavedSearch) => void;
  onClear: () => void;
}) {
  return (
    <div className="saved-searches-page">
      <PageHeader
        title="Recent searches"
        description="Queries you have run from the toolbar. Click one to search again."
        actions={
          searches.length > 0 ? (
            <button type="button" onClick={onClear}>
              Clear all
            </button>
          ) : undefined
        }
      />

      {searches.length === 0 ? (
        <EmptyState
          icon="search"
          title="No recent searches"
          description="Search from the toolbar — dog on a beach, or camera:iphone rating>3 — and it will show up here."
        />
      ) : (
        <ul className="saved-search-list">
          {searches.map((search) => (
            <li key={search.id} className="saved-search-card">
              <button
                type="button"
                className="saved-search-open"
                onClick={() => onRun(search)}
              >
                <strong>{search.query}</strong>
              </button>
              <div className="saved-search-actions">
                <button
                  type="button"
                  className="danger"
                  onClick={() => onDelete(search)}
                >
                  Remove
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
