import type { Person } from "../../lib/tauri";

/** Name a person, or merge them into an existing named cluster. */
export function PersonModal({
  person,
  people,
  name,
  onNameChange,
  onClose,
  onSubmit,
  onMergeInto,
}: {
  person: Person;
  people: Person[];
  name: string;
  onNameChange: (name: string) => void;
  onClose: () => void;
  onSubmit: () => void;
  onMergeInto: (intoId: string) => void;
}) {
  const others = people.filter(
    (p) => p.id !== person.id && (p.name?.trim() || p.faceCount > 0),
  );

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>{person.name?.trim() ? "Rename person" : "Name person"}</h2>
        <p className="muted">
          Naming makes this person searchable. You can also merge this cluster
          into someone you already named.
        </p>
        <label className="modal-label">
          Name
          <input
            type="text"
            autoFocus
            value={name}
            placeholder="e.g. Alex"
            onChange={(e) => onNameChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onSubmit();
            }}
          />
        </label>
        <div className="modal-actions">
          <button type="button" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="primary" onClick={onSubmit}>
            Save name
          </button>
        </div>
        {others.length > 0 && (
          <div className="modal-list">
            <p className="muted">Or merge into existing:</p>
            {others.map((p) => (
              <button key={p.id} type="button" onClick={() => onMergeInto(p.id)}>
                {p.name?.trim() || "Unnamed"} ({p.faceCount})
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
