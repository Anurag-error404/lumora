import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { fileSrc, type Album } from "../../lib/tauri";

/** Hero header shown above the asset grid when an album is open. */
export function AlbumDetailHeader({
  album,
  hasSelection,
  onBack,
  onStartPicking,
  onOpenMoveAlbum,
}: {
  album: Album | null;
  hasSelection: boolean;
  onBack: () => void;
  onStartPicking: (album: Album) => void;
  onOpenMoveAlbum: () => void;
}) {
  return (
    <div className="album-detail-head">
      {album &&
        (() => {
          const cover = album.coverThumbnailPath
            ? fileSrc(album.coverThumbnailPath)
            : null;
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
                  Add photos from your library or lock the whole album from the
                  albums grid.
                </p>
                <div className="album-detail-actions">
                  <button
                    className="primary"
                    onClick={() => onStartPicking(album)}
                  >
                    Add photos from library
                  </button>
                  {hasSelection && (
                    <button onClick={onOpenMoveAlbum}>
                      Add selection to album…
                    </button>
                  )}
                </div>
              </div>
            </div>
          );
        })()}
    </div>
  );
}
