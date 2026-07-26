# PhotoVault AI MVP — Task List

Track implementation against `tasks/plan.md`. Check items when acceptance criteria are met.

## Phase 0 — Foundation

- [x] **T01** Local logging + AppState paths
  - Acceptance: logs under app data on startup; paths resolvable in tests
  - Verify: `cd src-tauri && cargo test logging`
- [x] **T02** SQLite + migrations (MVP schema + FTS stub)
  - Acceptance: fresh DB migrates; reopen applies none; no faces/embeddings tables
  - Verify: `cargo test db_migrations`
- [x] **T03** Typed IPC shell + strip demo UI
  - Acceptance: typecheck passes; empty library shell; typed wrappers only
  - Verify: `bun run typecheck` + `bun run tauri dev` smoke

### Checkpoint A
- [x] `cargo test` + `bun run typecheck` pass
- [x] App opens, logs to disk, empty DB created

## Phase 1 — Import → browse

- [x] **T04** import_folder scan/hash/insert
- [x] **T05** list_assets + virtualised grid
- [x] **T06** Thumbnail pipeline + grid display

### Checkpoint B
- [x] E2E: pick folder → assets → thumbs → grid; originals unmodified

## Phase 2 — Watch + indexer

- [x] **T07** Folder watcher
- [x] **T08** Indexer queue, throttle, progress

### Checkpoint C
- [x] Watch + throttle; no telemetry

## Phase 3 — Organisation

- [x] **T09** Favourite / rating / colour label
- [x] **T10** Tags
- [x] **T11** Albums CRUD + drag-drop
- [x] **T12** Timeline year/month

### Checkpoint D
- [x] Albums/tags/ratings/timeline work on imported set

## Phase 4 — Search

- [x] **T13** FTS5 text search
- [x] **T14** Metadata filter parser

### Checkpoint E
- [x] Locate asset via search/filters on warm index

## Phase 5 — Duplicates + trash

- [x] **T15** Exact + near duplicates
- [x] **T16** Trash / restore / purge / undo

### Checkpoint F
- [x] Duplicate cleanup uses trash; undo/restore verified

## Phase 6 — QoL + acceptance

- [x] **T17** Shortcuts + lightbox
- [x] **T18** Theme + Recently added
- [x] **T19** Perf smoke docs + ADRs

### Checkpoint G — MVP complete
- [x] SPEC MVP checklist (minus deferred editing/saved search)
- [x] Zero network for core flows; logs local only
