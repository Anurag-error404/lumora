import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { fileSrc, type PlaceGroup } from "../../lib/tauri";
import { PlacesMap } from "./PlacesMap";

function placeCountLabel(count: number) {
  return `${count} ${count === 1 ? "photo" : "photos"}`;
}

/** Discover → Places: offline geometry map + GPS reverse-geocoded location grid. */
export function PlacesView({
  places,
  onOpenPlace,
  onRefresh,
}: {
  places: PlaceGroup[];
  onOpenPlace: (label: string) => void;
  onRefresh: () => void;
}) {
  return (
    <div className="people-page places-page">
      <PageHeader
        title="Places"
        description="Photos with GPS are grouped by location and shown on an offline map. Reverse geocoding and map geometry stay on-device — no coordinate or tile request leaves your machine."
        actions={
          <button type="button" onClick={onRefresh}>
            Refresh
          </button>
        }
      />
      {places.length === 0 ? (
        <EmptyState
          icon="place"
          title="No places yet"
          description="Import photos that carry GPS EXIF and LUMORA groups them here in the background. Location data is resolved offline."
        />
      ) : (
        <>
          <PlacesMap places={places} onOpenPlace={onOpenPlace} />
          <div className="place-cover-grid">
            {places.map((place) => (
              <article key={place.label} className="place-cover-card">
                <button
                  type="button"
                  className="place-cover-open"
                  onClick={() => onOpenPlace(place.label)}
                >
                  <div className="place-cover-media">
                    <SafeImage
                      src={
                        place.coverThumbnailPath
                          ? fileSrc(place.coverThumbnailPath)
                          : null
                      }
                      alt=""
                      loading="lazy"
                      fallback={<MediaFallback type="album" />}
                    />
                  </div>
                  <div className="place-cover-info">
                    <span className="place-cover-name">
                      {place.label}
                      {place.country ? (
                        <span className="muted"> · {place.country}</span>
                      ) : null}
                    </span>
                    <span className="place-cover-count muted">
                      {placeCountLabel(place.assetCount)}
                    </span>
                  </div>
                </button>
              </article>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
