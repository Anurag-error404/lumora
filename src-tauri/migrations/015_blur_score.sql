-- Blur score (variance of Laplacian) for soft-focus / out-of-focus review.
ALTER TABLE assets ADD COLUMN blur_score REAL;
CREATE INDEX IF NOT EXISTS idx_assets_blur_score
  ON assets(blur_score)
  WHERE blur_score IS NOT NULL AND deleted_at IS NULL;
