import type { Tag } from "../../lib/tauri";

/** Tag-selection modal: create a new tag or apply an existing one. */
export function TagModal({
  selectedCount,
  tags,
  name,
  onNameChange,
  onClose,
  onSubmit,
  onApplyExisting,
}: {
  selectedCount: number;
  tags: Tag[];
  name: string;
  onNameChange: (name: string) => void;
  onClose: () => void;
  onSubmit: () => void;
  onApplyExisting: (tag: Tag) => void;
}) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Tag selection</h2>
        <p className="muted">Apply a tag to {selectedCount} photo(s).</p>
        <label className="modal-label">
          Tag name
          <input
            type="text"
            autoFocus
            value={name}
            placeholder="e.g. passport"
            onChange={(e) => onNameChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onSubmit();
            }}
          />
        </label>
        <div className="modal-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={onSubmit}>
            Create & apply
          </button>
        </div>
        {tags.length > 0 && (
          <div className="modal-list">
            <p className="muted">Or apply an existing tag:</p>
            {tags.map((tag) => (
              <button key={tag.id} onClick={() => onApplyExisting(tag)}>
                {tag.name} ({tag.assetCount})
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
