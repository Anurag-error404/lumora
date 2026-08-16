import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type { PlaceGroup } from "../../lib/tauri";
import {
  panViewBox,
  viewBoxCenter,
  zoomViewBox,
  type ViewBox,
} from "./mapViewBox";
import { WORLD_LAND_PATH } from "./worldLandPath";

type Props = {
  places: PlaceGroup[];
  onOpenPlace: (label: string) => void;
};

const ZOOM_STEP = 1.25;
const PAN_THRESHOLD_PX = 4;

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

function clientToSvg(
  svg: SVGSVGElement,
  clientX: number,
  clientY: number,
  view: ViewBox,
) {
  const rect = svg.getBoundingClientRect();
  return {
    x: view.x + ((clientX - rect.left) / rect.width) * view.w,
    y: view.y + ((clientY - rect.top) / rect.height) * view.h,
  };
}

/** Offline geometry map: Natural Earth land + place pins. No tile network. */
export function PlacesMap({ places, onOpenPlace }: Props) {
  const fitted = useMemo(() => fitViewBox(places), [places]);
  const [view, setView] = useState(fitted);
  const [focusLabel, setFocusLabel] = useState<string | null>(null);
  const [panning, setPanning] = useState(false);
  const svgRef = useRef<SVGSVGElement>(null);
  const viewRef = useRef(view);
  const dragRef = useRef<{
    pointerId: number;
    x: number;
    y: number;
    view: ViewBox;
  } | null>(null);
  const didPanRef = useRef(false);

  viewRef.current = view;
  useEffect(() => {
    setView(fitted);
  }, [fitted]);

  useEffect(() => {
    const el = svgRef.current;
    if (!el) return;
    function onWheel(e: WheelEvent) {
      e.preventDefault();
      const svg = svgRef.current;
      if (!svg) return;
      const current = viewRef.current;
      const anchor = clientToSvg(svg, e.clientX, e.clientY, current);
      const factor = e.deltaY > 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
      setView(zoomViewBox(current, factor, anchor.x, anchor.y));
    }
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const grid = useMemo(() => {
    const meridians: number[] = [];
    const parallels: number[] = [];
    for (let lon = -180; lon <= 180; lon += 30) meridians.push(lon);
    for (let lat = -60; lat <= 60; lat += 30) parallels.push(lat);
    return { meridians, parallels };
  }, []);

  function zoomBy(factor: number) {
    const c = viewBoxCenter(view);
    setView(zoomViewBox(view, factor, c.x, c.y));
  }

  function onPointerDown(e: ReactPointerEvent<SVGSVGElement>) {
    if (e.button !== 0) return;
    didPanRef.current = false;
    dragRef.current = {
      pointerId: e.pointerId,
      x: e.clientX,
      y: e.clientY,
      view,
    };
  }

  function onPointerMove(e: ReactPointerEvent<SVGSVGElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    const svg = e.currentTarget;
    const rect = svg.getBoundingClientRect();
    const dxPx = e.clientX - drag.x;
    const dyPx = e.clientY - drag.y;
    if (!didPanRef.current) {
      if (Math.hypot(dxPx, dyPx) < PAN_THRESHOLD_PX) return;
      didPanRef.current = true;
      setPanning(true);
      svg.setPointerCapture(e.pointerId);
    }
    const dx = -(dxPx / rect.width) * drag.view.w;
    const dy = -(dyPx / rect.height) * drag.view.h;
    setView(panViewBox(drag.view, dx, dy));
  }

  function onPointerUp(e: ReactPointerEvent<SVGSVGElement>) {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== e.pointerId) return;
    dragRef.current = null;
    setPanning(false);
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  }

  function onMapKeyDown(e: ReactKeyboardEvent<SVGSVGElement>) {
    const stepX = view.w * 0.12;
    const stepY = view.h * 0.12;
    if (e.key === "+" || e.key === "=") {
      e.preventDefault();
      zoomBy(1 / ZOOM_STEP);
      return;
    }
    if (e.key === "-" || e.key === "_") {
      e.preventDefault();
      zoomBy(ZOOM_STEP);
      return;
    }
    if (e.key === "0") {
      e.preventDefault();
      setView(fitted);
      return;
    }
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      setView(panViewBox(view, -stepX, 0));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      setView(panViewBox(view, stepX, 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setView(panViewBox(view, 0, -stepY));
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setView(panViewBox(view, 0, stepY));
    }
  }

  function openPlace(label: string) {
    if (didPanRef.current) {
      didPanRef.current = false;
      return;
    }
    onOpenPlace(label);
  }

  const vb = `${view.x} ${view.y} ${view.w} ${view.h}`;

  return (
    <div className="places-map" role="region" aria-label="Places map">
      <div className="places-map-frame">
        <svg
          ref={svgRef}
          className={panning ? "places-map-svg is-panning" : "places-map-svg"}
          viewBox={vb}
          role="application"
          tabIndex={0}
          aria-label={`${places.length} ${places.length === 1 ? "place" : "places"} on map. Drag to pan, scroll or use + − to zoom, arrows to pan.`}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onKeyDown={onMapKeyDown}
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
                  onClick={() => openPlace(place.label)}
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
        <div className="places-map-controls">
          <button
            type="button"
            className="places-map-ctrl"
            aria-label="Zoom in"
            onClick={() => zoomBy(1 / ZOOM_STEP)}
          >
            +
          </button>
          <button
            type="button"
            className="places-map-ctrl"
            aria-label="Zoom out"
            onClick={() => zoomBy(ZOOM_STEP)}
          >
            −
          </button>
          <button
            type="button"
            className="places-map-ctrl places-map-ctrl-fit"
            aria-label="Reset map view"
            onClick={() => setView(fitted)}
          >
            Fit
          </button>
        </div>
      </div>
      <p className="places-map-caption muted">
        Drag to pan, scroll to zoom. Pins open a place. Offline — no tile
        network.
      </p>
    </div>
  );
}
