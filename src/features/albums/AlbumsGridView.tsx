import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { fileSrc, type Album } from "../../lib/tauri";

/** The Albums overview: cover grid plus create action. */
export function AlbumsGridView({
  albums,
  onCreateAlbum,
  onOpenAlbum,
}: {
  albums: Album[];
  onCreateAlbum: () => void;
  onOpenAlbum: (albumId: string) => void;
}) {
  return (
    <div className="albums-page">
      <PageHeader
        title="Albums"
        description="Group related photos into collections you can reopen, grow, or lock privately."
        actions={
          <button className="primary" onClick={onCreateAlbum}>
            New album
          </button>
        }
      />
      {albums.length === 0 ? (
        <EmptyState
          icon="album"
          title="Create your first album"
          description="Bring related photos together into a collection you can revisit and grow."
          action={{ label: "New album", onClick: onCreateAlbum }}
        />
      ) : (
        <div className="album-cover-grid">
          {albums.map((album) => {
            const cover = album.coverThumbnailPath
              ? fileSrc(album.coverThumbnailPath)
              : null;
            return (
              <article key={album.id} className="album-cover-card">
                <button
                  type="button"
                  className="album-cover-open"
                  onClick={() => onOpenAlbum(album.id)}
                >
                  <div className="album-cover-media">
                    <SafeImage
                      src={cover}
                      alt=""
                      loading="lazy"
                      fallback={<MediaFallback type="album" />}
                    />
                  </div>
                  <div className="album-cover-info">
                    <span className="album-cover-name">{album.name}</span>
                    <span className="muted">
                      {album.assetCount}{" "}
                      {album.assetCount === 1 ? "item" : "items"}
                    </span>
                  </div>
                </button>
              </article>
            );
          })}
        </div>
      )}
    </div>
  );
}
