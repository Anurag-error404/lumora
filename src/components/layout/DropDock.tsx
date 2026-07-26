import type { Dispatch, SetStateAction } from "react";
import { Icon } from "../icons";
import { MediaFallback } from "../MediaFallback";
import { SafeImage } from "../SafeImage";
import { fileSrc, type Album } from "../../lib/tauri";

/** Floating dock shown while dragging photos: drop onto an album or create one. */
export function DropDock({
  draggingCount,
  albums,
  dropAlbumId,
  setDropAlbumId,
  onDropAlbum,
  onDropNew,
}: {
  draggingCount: number;
  albums: Album[];
  dropAlbumId: string | null;
  setDropAlbumId: Dispatch<SetStateAction<string | null>>;
  onDropAlbum: (album: Album) => void;
  onDropNew: () => void;
}) {
  return (
    <div className="drop-dock" role="dialog" aria-label="Drop onto an album">
      <p className="drop-dock-title">
        Drop {draggingCount} photo{draggingCount > 1 ? "s" : ""}{" "}
        onto an album
      </p>
      <div className="drop-dock-list">
        {albums.map((album) => {
          const cover = album.coverThumbnailPath
            ? fileSrc(album.coverThumbnailPath)
            : null;
          return (
            <div
              key={album.id}
              className={`drop-dock-item ${
                dropAlbumId === album.id ? "over" : ""
              }`}
              onDragOver={(e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = "copy";
                setDropAlbumId(album.id);
              }}
              onDragLeave={() =>
                setDropAlbumId((cur) => (cur === album.id ? null : cur))
              }
              onDrop={(e) => {
                e.preventDefault();
                onDropAlbum(album);
              }}
            >
              <div className="drop-dock-cover">
                <SafeImage
                  src={cover}
                  alt=""
                  loading="lazy"
                  fallback={<MediaFallback type="album" compact />}
                />
              </div>
              <div className="drop-dock-meta">
                <span className="drop-dock-name">{album.name}</span>
                <span className="muted">
                  {album.assetCount}{" "}
                  {album.assetCount === 1 ? "item" : "items"}
                </span>
              </div>
            </div>
          );
        })}
        <div
          className={`drop-dock-item new ${
            dropAlbumId === "__new__" ? "over" : ""
          }`}
          onDragOver={(e) => {
            e.preventDefault();
            e.dataTransfer.dropEffect = "copy";
            setDropAlbumId("__new__");
          }}
          onDragLeave={() =>
            setDropAlbumId((cur) => (cur === "__new__" ? null : cur))
          }
          onDrop={(e) => {
            e.preventDefault();
            onDropNew();
          }}
        >
          <div className="drop-dock-cover new">
            <Icon name="album" />
          </div>
          <div className="drop-dock-meta">
            <span className="drop-dock-name">New album…</span>
            <span className="muted">Create from dropped photos</span>
          </div>
        </div>
      </div>
    </div>
  );
}
