import type { Album } from "../../lib/tauri";

/** Create-album / move-to-album modal. */
export function AlbumModal({
  mode,
  selectedCount,
  albums,
  name,
  onNameChange,
  onClose,
  onSubmit,
  onPickExisting,
}: {
  mode: "create" | "move";
  selectedCount: number;
  albums: Album[];
  name: string;
  onNameChange: (name: string) => void;
  onClose: () => void;
  onSubmit: () => void;
  onPickExisting: (albumId: string) => void;
}) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>{mode === "create" ? "New album" : "Move to album"}</h2>
        <p className="muted">
          {mode === "move"
            ? `${selectedCount} photo(s) selected — pick an existing album or create a new one.`
            : selectedCount
              ? `Optional: ${selectedCount} selected photo(s) will be added.`
              : "Create an empty album, then you’ll pick photos from the library."}
        </p>
        <label className="modal-label">
          {mode === "move" ? "New album name" : "Album name"}
          <input
            type="text"
            autoFocus
            value={name}
            placeholder="e.g. Trip to Goa"
            onChange={(e) => onNameChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") onSubmit();
            }}
          />
        </label>
        <div className="modal-actions">
          <button onClick={onClose}>Cancel</button>
          <button className="primary" onClick={onSubmit}>
            {mode === "move" ? "Create & move" : "Create album"}
          </button>
        </div>
        {mode === "move" && albums.length > 0 && (
          <div className="modal-list">
            <p className="muted">Or add to existing:</p>
            {albums.map((a) => (
              <button key={a.id} onClick={() => onPickExisting(a.id)}>
                {a.name} ({a.assetCount})
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
