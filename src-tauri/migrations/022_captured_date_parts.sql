-- Denormalized capture date parts for indexable timeline / memories queries.
-- strftime() in WHERE/GROUP BY cannot use idx_assets_captured_at; these columns can.

ALTER TABLE assets ADD COLUMN captured_ym TEXT;
ALTER TABLE assets ADD COLUMN captured_md TEXT;

UPDATE assets SET
  captured_ym = strftime('%Y-%m', COALESCE(captured_at, created_at)),
  captured_md = strftime('%m-%d', COALESCE(captured_at, created_at));

CREATE INDEX IF NOT EXISTS idx_assets_captured_ym
  ON assets(captured_ym) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_assets_captured_md
  ON assets(captured_md) WHERE deleted_at IS NULL;
