import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { fileSrc, type Album } from "../../lib/tauri";

/** Hero header shown above the asset grid when an album is open. */
export function AlbumDetailHeader({
  album,
  hasSelection,
  hasVaults,
  lockBusy,
  onBack,
  onStartPicking,
  onOpenMoveAlbum,
  onRemoveFromAlbum,
  onCreateLockedVault,
  onAddToExistingVault,
  onDeleteAlbum,
}: {
  album: Album | null;
  hasSelection: boolean;
  hasVaults: boolean;
  lockBusy: boolean;
  onBack: () => void;
  onStartPicking: (album: Album) => void;
  onOpenMoveAlbum: () => void;
  onRemoveFromAlbum: () => void;
  onCreateLockedVault: () => void;
  onAddToExistingVault: (album: Album) => void;
  onDeleteAlbum: (album: Album) => void;
}) {
  return (
    <div className="album-detail-head">
      {album &&
        (() => {
          const cover = album.coverThumbnailPath
            ? fileSrc(album.coverThumbnailPath)
            : null;
          const canLock = album.assetCount > 0 && !lockBusy;
          return (
            <div className="album-detail-hero">
              <div className="album-detail-cover">
                <SafeImage
                  src={cover}
                  alt=""
                  loading="lazy"
                  fallback={<MediaFallback type="album" compact />}
                />
              </div>
              <div className="album-detail-meta">
                <button className="text-btn" onClick={onBack}>
                  ← All albums
                </button>
                <h2>{album.name}</h2>
                <p className="muted">
                  {album.assetCount}{" "}
                  {album.assetCount === 1 ? "item" : "items"} in this album.
                  Add photos from your library, or move this album into an
                  encrypted vault.
                </p>
                <div className="album-detail-actions">
                  <button
                    className="primary"
                    onClick={() => onStartPicking(album)}
                  >
                    Add photos from library
                  </button>
                  {hasSelection && (
                    <>
                      <button type="button" onClick={onRemoveFromAlbum}>
                        Remove from album
                      </button>
                      <button type="button" onClick={onOpenMoveAlbum}>
                        Add selection to album…
                      </button>
                    </>
                  )}
                  <button
                    type="button"
                    onClick={onCreateLockedVault}
                    disabled={lockBusy}
                    title="Create a new encrypted vault"
                  >
                    Create locked vault
                  </button>
                  <button
                    type="button"
                    onClick={() => onAddToExistingVault(album)}
                    disabled={!canLock || !hasVaults}
                    title={
                      !hasVaults
                        ? "Create a vault first"
                        : album.assetCount === 0
                          ? "Add photos before moving this album"
                          : `Move ${album.name} into an existing vault`
                    }
                  >
                    Add to existing vault
                  </button>
                  <button
                    type="button"
                    className="danger"
                    onClick={() => onDeleteAlbum(album)}
                    disabled={lockBusy}
                    title={`Delete ${album.name}`}
                  >
                    Delete album
                  </button>
                </div>
              </div>
            </div>
          );
        })()}
    </div>
  );
}
