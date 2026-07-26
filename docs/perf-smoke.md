# Performance smoke checklist

Do **not** claim 1M-scale success without recording numbers from a real run.

## Targets (SPEC)

| Metric | Goal |
| --- | --- |
| Cold startup | < 2 s |
| FTS / metadata search | < 100 ms (warm) |
| Thumbnail generation | ≥ 100 images/min (background) |
| Idle RAM | < 250 MB |
| Scale | Stable browse/search toward 1M assets |

## How to measure

1. **Cold start:** quit app, time from `bun run tauri dev` window paint (or release binary launch) to interactive UI.
2. **Search latency:** `cargo test --manifest-path src-tauri/Cargo.toml perf_smoke -- --ignored --nocapture`
3. **Thumbnails:** same harness prints `thumbs_per_min`.
4. **Idle RAM:** Activity Monitor / `ps` RSS for `photovault-ai` after idle grid with warmed thumbs.
5. **1M strategy:** synthetic seed script (future) inserting metadata rows without decoding every image; virtualised grid already paginates at 500.

## Recorded runs

| Date | Machine | Library size | Cold start | Search | Thumbs/min | Idle RAM | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-07-26 | Apple M3 Pro, 18 GB, macOS 26.5.2 | 100 synthetic PNGs (480×320) via `perf_smoke` | _not measured_ (dev session already warm) | **0.14 ms** FTS text (`Canon`); **0.14 ms** `camera:` filter (avg of 20 warm queries) | **878**/min (debug build, sequential) | **~112 MB** RSS (`target/debug/photovault-ai` idle during `tauri dev`) | Near-dup Hamming cluster **0.19 ms** on 100 hashes. Debug thumbs; release expected faster. FTS rebuild migration `004` required for text search JOINs. |

## Privacy

All logs stay under the OS app-data `logs/` directory. No telemetry.
