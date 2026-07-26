-- Phase 2: ignored people.
--
-- An ignored person keeps its faces and centroid so future detections still
-- match it — they just never surface in People, search, or the info panel.
-- That is what makes "don't recognise this face again" stick.

ALTER TABLE people ADD COLUMN ignored INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_people_ignored ON people(ignored);
