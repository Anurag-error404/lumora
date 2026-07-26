import { PageHeader } from "../../components/PageHeader";
import type { HistorySnapshot } from "../../lib/tauri";

/** Undo/redo stacks and the full operation history. */
export function ActivityView({
  history,
  onUndo,
  onRedo,
  onRefresh,
}: {
  history: HistorySnapshot | null;
  onUndo: () => void;
  onRedo: () => void;
  onRefresh: () => void;
}) {
  return (
    <div className="activity-panel">
      <PageHeader
        title="Activity"
        description="Undo recent library changes, redo what you reversed, and review everything LUMORA has done this session."
        actions={
          <>
            <button disabled={!history?.canUndo} onClick={onUndo}>
              Undo
            </button>
            <button disabled={!history?.canRedo} onClick={onRedo}>
              Redo
            </button>
            <button onClick={onRefresh}>Refresh</button>
          </>
        }
      />

      <section className="activity-section">
        <div className="activity-section-head">
          <h3>Stacks</h3>
        </div>
        <div className="activity-columns">
          <div>
            <h4>Undo stack</h4>
            {(history?.undoStack.length ?? 0) === 0 ? (
              <p className="muted">No undoable actions yet.</p>
            ) : (
              <ul className="activity-list">
                {history?.undoStack.map((entry) => (
                  <li key={`undo-${entry.id}`}>
                    <span className="activity-label">{entry.label}</span>
                    <span className="muted activity-meta">
                      {new Date(entry.createdAt).toLocaleString()}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <div>
            <h4>Redo stack</h4>
            {(history?.redoStack.length ?? 0) === 0 ? (
              <p className="muted">Nothing to redo.</p>
            ) : (
              <ul className="activity-list">
                {history?.redoStack.map((entry) => (
                  <li key={`redo-${entry.id}`}>
                    <span className="activity-label">{entry.label}</span>
                    <span className="muted activity-meta">
                      {new Date(entry.createdAt).toLocaleString()}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </section>

      <section className="activity-section">
        <div className="activity-section-head">
          <h3>All operations</h3>
        </div>
        {(history?.activity.length ?? 0) === 0 ? (
          <p className="muted">
            Actions like trash, favourites, and album changes will appear here.
          </p>
        ) : (
          <ul className="activity-list">
            {history?.activity.map((entry) => (
              <li key={entry.id}>
                <span className={`activity-kind kind-${entry.kind}`}>
                  {entry.kind}
                </span>
                <span className="activity-label">{entry.label}</span>
                <span className="muted activity-meta">
                  {new Date(entry.createdAt).toLocaleString()}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
