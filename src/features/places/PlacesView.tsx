import { EmptyState } from "../../components/EmptyState";
import { PageHeader } from "../../components/PageHeader";
import { MediaFallback } from "../../components/MediaFallback";
import { SafeImage } from "../../components/SafeImage";
import { fileSrc, type PlaceGroup } from "../../lib/tauri";

function placeCountLabel(count: number) {
  return `${count} ${count === 1 ? "photo" : "photos"}`;
}

/** Discover → Places: photos grouped by offline-reverse-geocoded location. */
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
    <div className="people-page">
      <PageHeader
        title="Places"
        description="Photos with GPS are grouped by location, reverse-geocoded entirely on-device. No coordinate ever leaves your machine."
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
        <div className="person-cover-grid">
          {places.map((place) => (
            <article key={place.label} className="person-cover-card">
              <button
                type="button"
                className="person-cover-open"
                onClick={() => onOpenPlace(place.label)}
              >
                <div className="person-cover-media">
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
                <div className="person-cover-info">
                  <span className="person-cover-name">
                    {place.label}
                    {place.country ? (
                      <span className="muted"> · {place.country}</span>
                    ) : null}
                  </span>
                  <span className="muted">
                    {placeCountLabel(place.assetCount)}
                  </span>
                </div>
              </button>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
