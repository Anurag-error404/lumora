-- Phase 2: Faces / People derived data.
--
-- Rebuildable: dropping these tables and the faces jobs leaves a working
-- Phase 1 library. FTS gains a `people` column so naming a person makes their
-- photos findable through existing search.

CREATE TABLE IF NOT EXISTS people (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT,
  cover_face_id TEXT,
  face_count INTEGER NOT NULL DEFAULT 0,
  centroid BLOB,
  centroid_count INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_people_name ON people(name);

CREATE TABLE IF NOT EXISTS faces (
  id TEXT PRIMARY KEY NOT NULL,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  person_id TEXT REFERENCES people(id) ON DELETE SET NULL,
  bbox_x REAL NOT NULL,
  bbox_y REAL NOT NULL,
  bbox_w REAL NOT NULL,
  bbox_h REAL NOT NULL,
  score REAL NOT NULL DEFAULT 0,
  embedding BLOB NOT NULL,
  crop_path TEXT,
  detected_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_faces_asset ON faces(asset_id);
CREATE INDEX IF NOT EXISTS idx_faces_person ON faces(person_id);

-- Rebuild FTS with people column. Content is repopulated from Rust after migrate.
DROP TABLE IF EXISTS assets_fts;

CREATE VIRTUAL TABLE assets_fts USING fts5(
  asset_id UNINDEXED,
  filename,
  tags,
  camera,
  lens,
  ocr_text,
  people,
  tokenize='porter'
);
