import type { Album } from "../../lib/tauri";

/** Confirm album deletion; when the album has items, choose keep vs trash. */
export function DeleteAlbumDialog({
  album,
  busy,
  onCancel,
  onConfirm,
}: {
  album: Album;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (deleteAssets: boolean) => void;
}) {
  const count = album.assetCount;
  const itemLabel = count === 1 ? "item" : "items";

  return (
    <div
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="delete-album-title"
      onClick={onCancel}
    >
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2 id="delete-album-title">Delete “{album.name}”?</h2>
        <p className="muted">
          This album has {count} {itemLabel}. Choose what happens to those
          photos when the album is removed.
        </p>
        <div className="delete-album-options">
          <button
            type="button"
            disabled={busy}
            onClick={() => onConfirm(false)}
          >
            <strong>Keep photos in library</strong>
            <small className="muted">
              Remove the album only. Photos stay in your library.
            </small>
          </button>
          <button
            type="button"
            className="danger"
            disabled={busy}
            onClick={() => onConfirm(true)}
          >
            <strong>Delete photos too</strong>
            <small className="muted">
              Move all {count} {itemLabel} to trash, then delete the album.
            </small>
          </button>
        </div>
        <div className="modal-actions">
          <button type="button" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
