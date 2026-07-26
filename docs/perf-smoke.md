# Performance smoke checklist

Do **not** claim million-scale success without recording numbers from a real run.

## Targets

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
4. **Idle RAM:** Activity Monitor / `ps` RSS for the app process after an idle grid with warmed thumbs.
5. **1M strategy:** synthetic seed inserting metadata rows without decoding every image; virtualised grid already paginates.
6. **Import wall-clock:** import a real folder; check the toast (`files/s`), **Activity**, and **Developer → Import performance**. All local-only — no telemetry.

## Recorded runs

| Date | Machine | Library size | Cold start | Search | Thumbs/min | Idle RAM | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-07-26 | Apple M3 Pro, 18 GB, macOS 26.5.2 | 100 synthetic PNGs (480×320) via `perf_smoke` | _not measured_ (dev session already warm) | **0.14 ms** FTS text (`Canon`); **0.14 ms** `camera:` filter (avg of 20 warm queries) | **878**/min (debug build, sequential) | **~112 MB** RSS (debug binary idle during `tauri dev`) | Near-dup Hamming cluster **0.19 ms** on 100 hashes. Debug thumbs; release expected faster. |
| 2026-07-26 | _(import path)_ | — | — | — | — | — | Import: one image decode per file (thumb + aHash), parallel prepare workers, cancelable. Video thumbs use system ffmpeg when available. |

## Privacy

All logs stay under the OS app-data `logs/` directory. Import analytics stay in `import_runs` + Activity. No telemetry.
