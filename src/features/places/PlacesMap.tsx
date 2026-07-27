import { useMemo, useState } from "react";
import type { PlaceGroup } from "../../lib/tauri";
import { WORLD_LAND_PATH } from "./worldLandPath";

type Props = {
  places: PlaceGroup[];
  onOpenPlace: (label: string) => void;
};

type ViewBox = { x: number; y: number; w: number; h: number };

/** Project stored WGS84 into SVG space where y increases south (`y = -lat`). */
function toSvg(lat: number, lon: number) {
  return { x: lon, y: -lat };
}

function fitViewBox(places: PlaceGroup[]): ViewBox {
  if (places.length === 0) {
    return { x: -180, y: -90, w: 360, h: 180 };
  }

  let minLon = Infinity;
  let maxLon = -Infinity;
  let minLat = Infinity;
  let maxLat = -Infinity;
  for (const p of places) {
    minLon = Math.min(minLon, p.lon);
    maxLon = Math.max(maxLon, p.lon);
    minLat = Math.min(minLat, p.lat);
    maxLat = Math.max(maxLat, p.lat);
  }

  let lonSpan = Math.max(maxLon - minLon, 12);
  let latSpan = Math.max(maxLat - minLat, 8);
  const padLon = lonSpan * 0.35;
  const padLat = latSpan * 0.45;
  lonSpan += padLon * 2;
  latSpan += padLat * 2;

  let x = (minLon + maxLon) / 2 - lonSpan / 2;
  let yLatMax = (minLat + maxLat) / 2 + latSpan / 2; // north
  // Keep inside world bounds when possible.
  x = Math.max(-180, Math.min(x, 180 - lonSpan));
  const south = yLatMax - latSpan;
  if (south < -90) yLatMax = -90 + latSpan;
  if (yLatMax > 90) yLatMax = 90;

  return {
    x,
    y: -yLatMax,
    w: Math.min(lonSpan, 360),
    h: Math.min(latSpan, 180),
  };
}

function markerRadius(count: number, view: ViewBox) {
  const base = Math.max(view.w, view.h) * 0.012;
  const boost = Math.min(Math.log10(count + 1), 2) * base * 0.55;
  return Math.max(base + boost, view.w * 0.006);
}

function placeCountLabel(count: number) {
  return `${count} ${count === 1 ? "photo" : "photos"}`;
}

/** Offline geometry map: Natural Earth land + place pins. No tile network. */
export function PlacesMap({ places, onOpenPlace }: Props) {
  const view = useMemo(() => fitViewBox(places), [places]);
  const [focusLabel, setFocusLabel] = useState<string | null>(null);

  const grid = useMemo(() => {
    const meridians: number[] = [];
    const parallels: number[] = [];
    for (let lon = -180; lon <= 180; lon += 30) meridians.push(lon);
    for (let lat = -60; lat <= 60; lat += 30) parallels.push(lat);
    return { meridians, parallels };
  }, []);

  const vb = `${view.x} ${view.y} ${view.w} ${view.h}`;

  return (
    <div className="places-map" role="region" aria-label="Places map">
      <svg
        className="places-map-svg"
        viewBox={vb}
        role="img"
        aria-label={`${places.length} ${places.length === 1 ? "place" : "places"} on map`}
      >
        <rect
          className="places-map-ocean"
          x={-180}
          y={-90}
          width={360}
          height={180}
        />
        <g className="places-map-grid" aria-hidden="true">
          {grid.meridians.map((lon) => (
            <line key={`m-${lon}`} x1={lon} y1={-90} x2={lon} y2={90} />
          ))}
          {grid.parallels.map((lat) => (
            <line
              key={`p-${lat}`}
              x1={-180}
              y1={-lat}
              x2={180}
              y2={-lat}
            />
          ))}
        </g>
        <path
          className="places-map-land"
          d={WORLD_LAND_PATH}
          aria-hidden="true"
        />
        {places.map((place) => {
          const { x, y } = toSvg(place.lat, place.lon);
          const r = markerRadius(place.assetCount, view);
          const active = focusLabel === place.label;
          const label = place.country
            ? `${place.label} · ${place.country}`
            : place.label;
          return (
            <g key={place.label} className="places-map-marker-group">
              <circle
                className={
                  active
                    ? "places-map-marker places-map-marker-active"
                    : "places-map-marker"
                }
                cx={x}
                cy={y}
                r={r}
                role="button"
                tabIndex={0}
                aria-label={`${label}, ${placeCountLabel(place.assetCount)}`}
                onClick={() => onOpenPlace(place.label)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onOpenPlace(place.label);
                  }
                }}
                onFocus={() => setFocusLabel(place.label)}
                onBlur={() =>
                  setFocusLabel((cur) => (cur === place.label ? null : cur))
                }
                onMouseEnter={() => setFocusLabel(place.label)}
                onMouseLeave={() =>
                  setFocusLabel((cur) => (cur === place.label ? null : cur))
                }
              >
                <title>{`${label} — ${placeCountLabel(place.assetCount)}`}</title>
              </circle>
              {active ? (
                <text
                  className="places-map-label"
                  x={x}
                  y={y - r * 1.8}
                  textAnchor="middle"
                  fontSize={Math.max(view.h * 0.045, 2.2)}
                >
                  {place.label}
                </text>
              ) : null}
            </g>
          );
        })}
      </svg>
      <p className="places-map-caption muted">
        Offline map — place pins from GPS EXIF. No tile network.
      </p>
    </div>
  );
}
