# ADR 0001: MVP media stack decisions

## Status

Accepted (Phase 1). Partially superseded for video thumbnails — see Consequences.

## Context

Early product work needed concrete media-stack choices before indexing and search could ship.

## Decision

1. **EXIF:** `kamadak-exif` (pure Rust) for reads only. EXIF write deferred.
2. **Thumbnails:** JPEG, max 320px long edge, under app-data thumbs keyed by content hash. No eviction policy yet.
3. **Video:** Index in the same library as photos. Frame extraction uses **system ffmpeg / ffprobe** when present on `PATH` (soft-fail to a placeholder if missing). ffmpeg is not bundled.
4. **Near-duplicates:** Average hash (aHash) over an 8×8 luma preview — not ML. Current UI threshold is Hamming ≤ 2.
5. **Editing / saved searches:** Shipped later than the initial MVP cut; present in the app today.

## Consequences

- No system `exiftool` dependency for reads.
- Contributors need ffmpeg only if they care about video thumbs in development.
- Thumbnail disk use grows with library size; measure in [`../perf-smoke.md`](../perf-smoke.md).
- Near-duplicate false positives remain possible with aHash; cleanup UI keeps exact vs near segregated for that reason.
