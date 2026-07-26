-- Phase 2: Places derived data.
--
-- Rebuildable: one row per geotagged asset, derived from GPS EXIF and an
-- offline reverse-geocode. Dropping this table (and the `places` jobs) leaves a
-- fully working Phase 1 library — Places simply disappears until reprocessed.

CREATE TABLE IF NOT EXISTS asset_places (
  asset_id TEXT PRIMARY KEY NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  lat REAL NOT NULL,
  lon REAL NOT NULL,
  -- Human-readable "City, Region" from offline reverse geocoding. Null when a
  -- coordinate could not be resolved to a known place.
  place_label TEXT,
  -- ISO country code from the nearest GeoNames record.
  country TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_asset_places_label ON asset_places(place_label);
