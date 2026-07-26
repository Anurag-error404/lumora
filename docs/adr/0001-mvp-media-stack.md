# ADR 0001: MVP media stack decisions

## Status

Accepted (Phase 1)

## Context

SPEC open questions needed concrete choices before indexing/search work.

## Decision

1. **EXIF:** `kamadak-exif` (pure Rust) for reads only. EXIF write deferred.
2. **Thumbnails:** JPEG, max 320px long edge, stored at `app_data/thumbs/{sha256}.jpg`. No eviction in MVP.
3. **Video:** Index in the same library with placeholder thumbnails; frame extraction deferred (no ffmpeg bundle).
4. **Near-duplicates:** Simple average hash (aHash) over an 8×8 luma thumbnail — not ML.
5. **Editing / saved searches:** Out of MVP (v1.5 / Phase 1.1).

## Consequences

- No system `exiftool` dependency.
- Video UX is browse/metadata-first until a later phase adds frame thumbs.
- Thumbnail disk use grows with library size; document and measure in `docs/perf-smoke.md`.
