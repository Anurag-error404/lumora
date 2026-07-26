/** Import chooser: individual files or a watched folder. */
export function ImportModal({
  onClose,
  onChooseFiles,
  onChooseFolder,
}: {
  onClose: () => void;
  onChooseFiles: () => void;
  onChooseFolder: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Import media</h2>
        <p className="muted">
          Add individual photos/videos, or import a whole folder (folders stay
          watched for new files).
        </p>
        <div className="modal-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={onChooseFiles}>
            Choose files…
          </button>
          <button onClick={onChooseFolder}>Choose folder…</button>
        </div>
      </div>
    </div>
  );
}
