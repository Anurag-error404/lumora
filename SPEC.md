# Spec: LUMORA (Phase 2 — on-device intelligence)

> **Current phase: 2.** Source of truth for active work. Derived from `prd.md`.
> Phase 1 (MVP) is **delivered** and is now the baseline — see
> [Phase 1 baseline](#phase-1-baseline-delivered). Do not implement features
> outside this spec until it is updated and approved.

## Objective

Build a **local-first desktop photo & video library manager** for **archive / professional users** whose primary need is **organising large collections** (up to **1 million** photos and videos) without cloud dependency.

**Product name:** LUMORA.  
**Tagline:** your memories your machine.  
**Why:** Deliver Google Photos–like library convenience offline, optimised for performance, privacy, and filesystem-scale indexing — not social or consumer sharing.

### Phase 2 objective

Make the library **understand its own contents on-device**. Everything a cloud
service does with your photos — recognising people, finding a shot by
describing it, reading text, grouping places — happens locally, with **no image
or embedding ever leaving the machine**.

Phase 2 succeeds when a user can find an asset by *what is in it*, not only by
filename, date, or a tag they remembered to add.

### Primary users

- Archive / pro users organising large local photo and video libraries.
- Privacy-first consumers become a **secondary** target in Phase 2: on-device
  intelligence is the differentiator against cloud photo services.

### Core user stories (Phase 2)

1. Search the library in plain language ("dog on a beach at sunset") with no cloud call.
2. See people grouped automatically, name a person once, and browse everything they appear in.
3. Find screenshots, documents, and receipts by the **text inside them**.
4. Get smart collections that are derived from content, not just filename patterns.
5. Browse by place when GPS EXIF is present.
6. Keep every model, embedding, and index **on-device**, inspectable and deletable.

### Phase 1 baseline (delivered)

Import + watched folders, incremental indexing, virtualised grid, timeline,
albums/tags/favourites/ratings/colour labels, FTS5 + metadata filters, exact and
near-duplicate detection, trash/undo, light theme, local-only logs, **Home**,
**recent search history** (toolbar hints + Discover page), and a user-focused
**Settings** surface (General → About) separate from the engineering
**Developer** page. Full checklist in
[Phase 1 functional scope](#phase-1-functional-scope-delivered).

**Also delivered early (originally Phase 2):** the encrypted vault — Argon2id +
XChaCha20-Poly1305, recovery code, locked albums and folders, encrypted
metadata. Treated as complete, not as pending Phase 2 work.

**Also delivered early (Phase 2 slice):** on-device **CLIP semantic search** —
model registry, user-initiated download, background embedding, natural-language
search blended with FTS. See [Phase 2 checklist](#phase-2-checklist-active).

**Also delivered (Phase 2 slice):** on-device **OCR** — RapidOCR PP-OCRv4
(detection + recognition + charset) via the same `ort` runtime; extracted text
in `asset_text`, joined into FTS, powering Documents and Receipts smart
collections. Opt-in via Settings → AI Features.

**Also delivered (Phase 2 slice):** on-device **Faces / People** — InsightFace
buffalo_l (SCRFD-10G + ArcFace w600k_r50, non-commercial research licence);
background detection/clustering, People view with name/merge/detach/ignore, names
in FTS. Opt-in via Settings → AI Features.

### Non-goals (Phase 2)

- Cloud backup, sync, or multi-user collaboration.
- **Any** network dependency for core features, including model inference.
- Training or fine-tuning models on device — inference only.
- Full professional RAW editing or plugin system.
- Mobile companion app.
- AI captions / "memories" storytelling (Phase 3).
- Network telemetry, crash upload, or analytics of any kind.

---

## Tech Stack

| Layer | Choice | Notes |
| --- | --- | --- |
| Desktop shell | Tauri v2 | Small binary; Rust backend for FS/indexing |
| UI | React + TypeScript | Vite via Tauri scaffold |
| Package manager | **Bun** | Frontend deps and scripts |
| Backend | Rust | Indexer, thumbnails, search, DB access |
| Database | SQLite | Local library metadata |
| Search | SQLite FTS5 + CLIP vectors | Metadata / text search; semantic when models installed |
| File watching | `notify` (Rust) | Live folder updates |
| Metadata | `kamadak-exif` | EXIF read; EXIF write still deferred |
| Image processing (non-ML) | Rust image crates / platform decode | Thumbnails, dimensions |
| Crash / diagnostics | Local log files only | Never leave the machine |
| Vault | Argon2id + XChaCha20-Poly1305 | Delivered; recovery code, locked albums/folders |

### Phase 2 additions

| Layer | Choice | Notes |
| --- | --- | --- |
| ML runtime | **ONNX Runtime (`ort` crate)** | Shipped; inference only; CoreML on macOS, CPU fallback |
| Semantic search | CLIP ViT-B/32 image/text embeddings | Shipped; vectors in SQLite; brute-force scan |
| Faces | SCRFD-10G + ArcFace w600k_r50 (InsightFace buffalo_l) via `ort` | Shipped; People view + FTS names |
| OCR | RapidOCR PP-OCRv4 (det+rec+dict) via `ort` | Shipped; feeds FTS + Documents/Receipts |
| Vector index | SQLite blob + brute force | ANN only if measurement demands it |
| Model storage | App-data `models/` | User-visible, deletable; install from Settings |

**Platforms:** macOS first (primary target), then Windows and Linux.  
**Network:** Core features must work with **zero external network dependency**.
Phase 2 permits exactly one narrow exception — an **explicit, user-initiated
model download** — and only if approved per [Ask first](#ask-first). Inference,
indexing, and search must never touch the network.

---

## Commands

Scaffolded with `create-tauri-app` (Tauri v2, React + TypeScript, Bun). Canonical commands:

```
# Install (frontend / workspace)
bun install

# Dev (Tauri + Vite)
bun run tauri dev

# Build release
bun run tauri build

# Frontend only (Vite)
bun run dev
bun run build

# Typecheck
bun run typecheck

# Frontend unit / component tests (add when first tests land)
bun test

# Rust tests (from src-tauri)
cd src-tauri && cargo test

# Lint / format (add prettier/eslint scripts when configured; Rust ready now)
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

CI should run: `bun install`, `bun run typecheck`, `bun run build`, `bun test` (once tests exist), `cargo fmt --check`, `cargo clippy`, `cargo test`.

**CI status:** Automated CI pipelines are still **not required**. Local
verification with the commands above is sufficient. CI can be added when
releasing or collaborating.

**Phase 2 note:** ML tests must run without network access. Model-dependent
tests are `#[ignore]`d and gated on a locally installed model; the default
`cargo test` run must stay green on a machine with no models present.

---

## Project Structure

```
Master-Photo-Manager/
├── SPEC.md                 → This specification (living)
├── prd.md                  → Product vision & roadmap (broader than MVP)
├── package.json            → Bun scripts / frontend workspace
├── src/                    → React + TypeScript UI
│   ├── components/         → UI components
│   ├── features/           → Feature modules (library, albums, search, …)
│   ├── lib/                → Shared TS utilities, Tauri invoke wrappers
│   ├── styles/             → Global styles / design tokens
│   └── App.tsx
├── src-tauri/              → Rust / Tauri backend
│   ├── src/
│   │   ├── commands/       → Tauri command handlers
│   │   ├── indexer/        → Scan, hash, EXIF, incremental index
│   │   ├── thumbnails/     → Thumbnail generation & cache
│   │   ├── watcher/        → Folder watch
│   │   ├── search/         → FTS5 + filter queries
│   │   ├── saved_searches/ → Recent search history (auto-record, cap 30)
│   │   ├── preferences/    → User prefs JSON (Settings)
│   │   ├── edit/           → Rotate / crop / exposure (replace or copy)
│   │   ├── duplicates/     → SHA256 + perceptual hash
│   │   ├── db/             → SQLite schema, migrations, queries
│   │   ├── trash/          → Soft-delete / restore
│   │   ├── smart/          → Rule-based smart collections
│   │   ├── vault/          → Encrypted vault (delivered)
│   │   ├── ml/             → Model registry + ONNX inference (CLIP + OCR shipped)
│   │   ├── faces/          → SCRFD + ArcFace → cluster → people (shipped)
│   │   ├── semantic/       → CLIP embeddings + vector search (shipped)
│   │   ├── ocr/            → RapidOCR PP-OCRv4 text recognition → FTS (shipped)
│   │   └── logging/        → Local crash & diagnostic logs
│   ├── migrations/         → SQL migrations (current schema version **9**)
│   └── Cargo.toml
├── tests/                  → Frontend unit / integration tests
├── e2e/                    → Optional desktop E2E (later)
└── docs/                   → ADRs, architecture notes
```

**Still deferred (Phase 3+):** sync, plugins, mobile, non-destructive editing history.

**Data on disk (runtime, not in repo):**

- Original media: user-owned paths (unchanged by default).
- App data: SQLite DB, thumbnail cache, local logs, `preferences.json` under the
  OS app-data directory.
- **Phase 2:** ML models under app-data `models/`; embeddings in SQLite. All of
  it user-deletable without breaking the library — deleting derived data must
  only lose derived features.

---

## Code Style

### TypeScript / React

- Functional components; TypeScript strict mode.
- Feature folders over deep generic `components/` dumps.
- Tauri IPC typed in one place (`src/lib/tauri/` or similar); UI never calls raw `invoke` with stringly-typed payloads scattered around.
- Prefer Bun-native tooling for scripts; no Node-specific APIs without need.

```tsx
// Good: typed command wrapper + thin feature hook
import { invoke } from "@tauri-apps/api/core";

export type AssetId = string;

export async function listAssets(params: {
  limit: number;
  offset: number;
}): Promise<AssetSummary[]> {
  return invoke("list_assets", params);
}
```

### Rust

- Modules by domain (`indexer`, `db`, `search`), not by layer alone.
- Explicit error types; no `unwrap()` in production paths.
- Background work on a dedicated indexer queue with CPU throttling when the system is busy.
- Migrations are versioned SQL; never hand-edit the live DB schema in ad-hoc code.

```rust
// Good: command returns Result, domain owns the logic
#[tauri::command]
async fn list_assets(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> Result<Vec<AssetSummary>, AppError> {
    state.library.list_assets(limit, offset).await
}
```

### Naming

| Kind | Convention |
| --- | --- |
| React components | `PascalCase` |
| TS files (non-components) | `kebab-case` or `camelCase` matching existing scaffold |
| Rust modules / files | `snake_case` |
| DB tables / columns | `snake_case` |
| Tauri commands | `snake_case` verbs (`import_folder`, `list_assets`) |

### Formatting

- Prettier (TS) / `cargo fmt` (Rust); run locally before merging (CI optional — see Commands).

---

## Testing Strategy

| Level | Where | What |
| --- | --- | --- |
| Unit (TS) | `tests/` or colocated `*.test.ts` | Pure UI helpers, filter parsing, state helpers |
| Unit (Rust) | `src-tauri` `#[cfg(test)]` | Hashing, EXIF parsing fixtures, duplicate logic, SQL queries |
| Integration | Rust + temp dirs | Import folder → DB rows → FTS results |
| Manual / scale | Local fixtures | Smoke on small libs; **MVP acceptance scale: up to 1M assets** (photos + videos) |
| E2E | `e2e/` (optional post-MVP) | Critical browse/search paths |

**Coverage expectations (MVP):**

- Critical Rust paths (indexer, duplicates, search filters, trash/restore): high coverage with fixtures.
- UI: test interactive logic and IPC wrappers; pixel-perfect E2E not required for Phase 1.
- Performance checks are **acceptance tests**, not unit tests: measure cold start, search latency, idle RAM against targets below.

**ML testing (Phase 2):**

- Pure logic — clustering, vector maths, tokenisation, model-registry paths —
  is unit tested with **synthetic vectors**, no model file required.
- Real inference tests are `#[ignore]`d and gated behind a locally installed
  model. `cargo test` on a fresh clone with no models must still pass.
- Accuracy is validated against a small **hand-labelled fixture set** checked
  into the repo (thumbnails only, no personal media), not by eyeballing.

---

## Boundaries

### Always

- Preserve originals on disk; never move/rename/rewrite originals except via explicit user actions (edit, trash, rename).
- Keep all library data, logs, and crash reports **local**; no outbound telemetry or analytics.
- Run lint, typecheck, and relevant tests before considering a change done.
- Use Bun for JS/TS package management and scripts.
- Index incrementally; support watched folders.
- Soft-delete to trash with restore; make destructive flows reversible where specified.
- Virtualise large grids so 1M-scale libraries remain navigable.
- Write structured local logs for crashes and indexer failures.
- **Run ML inference on-device only**, on the background queue, never blocking
  the UI or the core indexer.
- **Degrade gracefully without models:** every Phase 1 feature keeps working
  when no model is installed. AI features are additive, never a hard dependency.
- **Keep derived data reversible:** embeddings, face clusters, and OCR text can
  be deleted and rebuilt without touching originals or library metadata.

### Ask first

- Adding any dependency (especially native/Rust crates or UI libraries).
- Schema / migration changes.
- Changing CI, release packaging, or supported OS minimums.
- **Any network access** — still default deny. The only candidate exception is
  an explicit, user-initiated model download; it needs sign-off before it ships.
- Which ML runtime and which specific model weights to adopt.
- Bundling model weights into the installer (size / licence implications).
- Moving originals into an app-managed store or changing trash semantics.
- Scope expansion into Phase 3 features (editing history, plugins, sync, mobile).

### Never

- Commit secrets, API keys, or user library paths with personal media.
- Commit model weights or personal media fixtures to the repo.
- Send telemetry, crash dumps, images, embeddings, or usage data off-device.
- Call a hosted inference API for any feature — inference is local or absent.
- Silently download anything, including models.
- Remove or skip failing tests to green CI without fixing the cause.
- Edit vendor / generated directories by hand.
- Implement cloud sync, multi-user sharing, or remote backup.

---

## Data model

### Phase 1 (delivered)

**Assets** (images + videos): `id`, `path`, `hash` (SHA256), `perceptual_hash`, `media_type`, `width`, `height`, `duration_ms` (video), `created_at`, `captured_at`, `favorite`, `hidden`, `rating`, `color_label`, `thumbnail_path`, plus EXIF-derived fields needed for filters (camera, lens, etc. as available).

**Albums:** `id`, `name`, `cover_asset_id`, `created_at`  
**Album assets:** `album_id`, `asset_id`  
**Tags / asset_tags:** many-to-many  
**Trash:** soft-delete metadata + purge after retention (default **30 days**)  
**FTS:** searchable filename, tags, and selected metadata fields  
**Watched folders:** `id`, `path`, `created_at`  
**Recently viewed:** `asset_views(asset_id, viewed_at)` — updated when the media viewer opens an asset  
**Recent searches:** `saved_searches(id, name, query, created_at, updated_at)` — auto-recorded
query history (table name historical; behaviour is recent-history, not manual “saved”
named searches). Cap **30**; upsert by query (case-insensitive); surfaced in the
toolbar hint dropdown and Discover → Recent searches.  
**Vault:** encrypted asset/album tables with wrapped keys and encrypted metadata  
**Preferences:** `preferences.json` in app data (not SQL) — Settings toggles for
appearance, AI, privacy metadata, performance hints, import/export defaults

### Phase 2 (tables — derived / rebuildable)

**Shipped:**

**`ml_models`:** `id`, `kind`, `version`, `path`, `sha256`, `installed_at` — the model registry  
**`asset_embeddings`:** `asset_id`, `model_id`, `vector` (blob), `dim`, `created_at`  
**`ml_jobs`:** per-asset per-model processing state so work is resumable and incremental  
**`asset_text`:** `asset_id`, `text`, `lang`, `confidence`, `created_at` — OCR output, joined into FTS (`assets_fts.ocr_text`)  
**`people`:** `id`, `name` (nullable), `cover_face_id`, `face_count`, `centroid`, `centroid_count`, `ignored`, `created_at`, `updated_at` — an ignored person keeps its centroid, so the same face keeps matching it and stays hidden  
**`faces`:** `id`, `asset_id`, `person_id`, `bbox_*`, `score`, `embedding` (blob), `crop_path`, `detected_at`

**Pending (places):**

**`asset_places`:** `asset_id`, `lat`, `lon`, `place_label` — from GPS EXIF, reverse-geocoded offline

### On-device model catalog (shipped)

| Bundle | Files | Approx. size | License | Status |
| --- | --- | --- | --- | --- |
| `clip-vit-b32` | CLIP image ONNX, text ONNX, tokenizer | ~600 MB | MIT | Shipped |
| `rapidocr-ppv4` | `ch_PP-OCRv4_det_infer.onnx`, `ch_PP-OCRv4_rec_infer.onnx`, `ppocr_keys_v1.txt` | ~15.6 MB | Apache-2.0 | Shipped |
| `insightface-buffalo-l` | `scrfd_10g_bnkps.onnx`, `arcface_w600k_r50.onnx` | ~191 MB | InsightFace (non-commercial research) | Shipped |

**Next:** MobileNetV4 auto-tags; captioning later (Florence-2).

**Invariant:** every Phase 2 table references `assets(id)` with cascade delete and
holds only derived data. Dropping all of them must leave a fully working Phase 1 library.

---

## Functional scope

## Phase 2 checklist (active)

### Model infrastructure

- [x] Model registry: install, verify checksum, list, remove; app works with zero models.
- [x] Inference runs on the existing background queue with throttling; never blocks UI.
- [x] Per-asset job state so indexing is resumable and incremental.
- [x] Settings surface: what is installed, disk used, reprocess / delete derived data.
  *(Settings → AI Features + Storage. Developer stays diagnostics-only.)*

### Semantic search

- [x] Image embeddings generated in the background for indexed photos.
- [x] Natural-language query → text embedding → vector similarity over the library.
- [x] Blends with existing FTS + filters rather than replacing them
      (structured tokens like `camera:` stay on FTS; plain language tries CLIP first).
- [x] Brute-force vector scan first; add an ANN index only if measurement demands it.
- [x] User can disable semantic search from Settings → AI Features.
- [x] Models downloadable / removable from Settings → AI Features (CLIP ViT-B/32).

### Faces / People

- [x] Detect faces (SCRFD), embed (ArcFace 512-d), and cluster without any naming required.
- [x] People view: browse clusters, name a person, merge / split clusters.
- [x] Naming a cluster makes that person searchable and filterable (FTS `people` column).
- [x] Ignore a person (People card, person header, or info-panel face chip): hidden from
      People, search, and the info panel, and future detections of that face stay hidden.
      Reversible from People → Ignored.
- [x] Face data is deletable in one action (Settings → Clear face data / remove models).
- [x] Models downloadable / removable from Settings → AI Features; face recognition
      toggle gates the background worker; invalidate on edit save.

### OCR / text-in-image

- [x] Extract text from screenshots and document-like images (RapidOCR PP-OCRv4).
- [x] Feed extracted text into FTS so existing search finds it.
- [x] Powers the Documents and Receipts smart collections.
- [x] Models downloadable / removable from Settings → AI Features; OCR toggle
      gates the background worker; clear text / invalidate on edit save.

### Smart collections (content-derived)

- [x] Videos, RAW photos, Screenshots (rule-based, delivered).
- [x] Documents — OCR/text-density driven (length + confidence floor).
- [x] Receipts — OCR keyword + currency-token driven.
- [x] Selfies, panoramas, and similar EXIF/geometry rules that need no model.
- [x] A single collection framework: rule-based and ML-backed collections share
      one definition, one query path, and one counting path.

### Places

- [ ] Places view from GPS EXIF, grouped by offline reverse geocoding.
- [ ] Map rendering only if an offline tile/geometry approach is agreed — no tile server calls.

### Phase 1 residual

- [x] Recent searches (auto-recorded history + toolbar hint dropdown).
- [ ] Video frame-extract thumbnails (still needs an ffmpeg decision).
- [ ] Record cold-start and large-library perf numbers (see success criteria).

### Settings & Developer (delivered)

- [x] Multi-page **Settings**: General, Appearance, Library, AI Features,
      Privacy & Security, Storage, Performance, Keyboard Shortcuts,
      Import & Export, Updates, About.
- [x] Preferences persist in app-data `preferences.json`.
- [x] Live prefs include: confirm-before-delete, double-click-to-open,
      thumbnail size / density / animations, semantic search toggle, OCR toggle,
      face recognition toggle, background embedding pause, library watch toggle,
      privacy metadata toggles, storage summary + clear/rebuild cache + optimize DB.
- [x] Settings shows only working controls — no disabled “Soon” placeholders.
- [x] **Developer** page is diagnostics-only (app/DB/indexer/logs/paths);
      storage actions and AI model management live under Settings.

---

## Phase 1 functional scope (delivered)

### Library

- [x] Import existing folders (recursive).
- [x] Watch folders for add/change/remove.
- [x] Incremental indexing (hash → EXIF → thumbnail).
- [x] Preserve originals; store only metadata + thumbnails in app data.
- [x] Support photos and videos in the same library (video thumbs = placeholders in MVP).

### Organisation

- [x] Manual albums (create, rename, add/remove, drag-drop).
- [x] Tags, favourites, 1–5 ratings, colour labels.
- [x] Timeline by year/month (Google Photos–style browsing).
- [x] Recently added.
- [x] Recently viewed (updated when opening the media viewer).

### Search

- [x] Text search via FTS5 (filename, tags, basic metadata).
- [x] Filter search (`camera:`, `rating>`, `before:`, `after:`, etc. as EXIF allows).
- [x] Recent searches — auto-recorded when a query settles or is submitted;
      Discover → Recent searches; toolbar combobox hints (↑↓ / Enter / click).
- [x] Semantic / natural-language search when CLIP models are installed
      (Phase 2 slice; see checklist above).

### Duplicates

- [x] Exact duplicates via SHA256.
- [x] Near-duplicates via perceptual hash with **Hamming-distance** clustering (similar images, not only identical hashes).
- [x] Safer cleanup flow with trash/undo.

### Editing (basic)

- [x] Rotate (90° steps), flip H/V, aspect-ratio crop (drag/resize + presets), and exposure (±2 EV) for library images.
- [x] **Save (replace original)** with confirmation, or **Save as copy** (sibling file).
- [x] After save: re-hash, regenerate thumbnail, clear CLIP embedding, queue re-embed.
- [x] Videos are not editable in this surface (open in an external app if needed).
- [ ] Prefer non-destructive edit history / sidecars — deferred to Phase 3.

### QoL

- [x] Keyboard shortcuts: space (preview), arrows, delete, favourite (fullscreen optional).
- [x] Trash with 30-day retention.
- [x] Undo for destructive library operations where feasible.
- [x] **Light theme only** (system/dark mode deferred; keep a single light appearance).
- [x] Local-only diagnostic logs.
- [x] Watched-folders management UI (list / add / remove).
- [x] Home overview surface.
- [x] Settings + Developer separation (see above).
- [x] Activity log and Exports history views.
- [x] ZIP export of selected assets.

### Library notes

- Video thumbnails are still **placeholders** (duration/container metadata indexed; frame extraction deferred — no ffmpeg yet).

### Delivered ahead of schedule

- Encrypted vault (Argon2id + XChaCha20-Poly1305, recovery code, locked albums and
  folders, encrypted metadata) — originally Phase 2, shipped during Phase 1.
- CLIP semantic search + model registry — Phase 2 first slice, shipped.

---

## Performance / success criteria

### Phase 1 bar (unchanged)

| Metric | Goal |
| --- | --- |
| Cold startup | < 2 seconds |
| Metadata / FTS search latency | < 100 ms on indexed data |
| Thumbnail generation | ≥ 100 images/minute in background |
| Idle RAM | < 250 MB |
| DB size | < 10 GB metadata for 1M assets (excluding originals & thumbnails policy TBD in plan) |
| Background indexing | Low priority; throttle when system busy |
| Indexing success rate | > 99% on supported formats |
| Scale | Stable browse/search with **1M** photos + videos |
| Privacy | Zero external network dependency for core features; logs stay local |
| UX | User can locate an asset within ~10 seconds via search or filters on a warmed index |

**Measurement debt:** cold start and 1M-scale runs are still unrecorded in
`docs/perf-smoke.md`. Phase 2 must not add ML load until those baselines exist.

### Phase 2 bar (new)

| Metric | Goal |
| --- | --- |
| Semantic query latency | < 300 ms on a 100k-asset library (warm) |
| Embedding throughput | ≥ 300 images/minute in background on the primary target machine |
| Face detection throughput | ≥ 200 images/minute in background |
| Idle RAM with models loaded | < 600 MB; models unload when idle |
| UI responsiveness under inference | Grid scroll stays interactive; inference yields to user actions |
| Model disk footprint | Reported to the user; removable at any time |
| Degradation | Zero models installed ⇒ every Phase 1 feature behaves exactly as before |
| Privacy | No network syscall during inference, indexing, or search |

---

## Roadmap alignment

| Phase | Status | Scope |
| --- | --- | --- |
| **1 — MVP** | **Delivered** | Import, watch (+ manage UI), thumbnails (video = placeholders), metadata, albums, favourites, tags, ratings, labels, FTS + filters, timeline, recently added/viewed, recent search history + toolbar hints, Home, Hamming near-dups, trash/undo, light theme only, local logs, Settings + Developer, basic image edit (rotate/crop/exposure), **plus encrypted vault** |
| **2 — On-device intelligence (this spec)** | **Active** | **Shipped:** model infra, CLIP semantic search (+ re-embed after edit), OCR (RapidOCR PP-OCRv4 → FTS + Documents/Receipts), Faces/People (InsightFace buffalo_l SCRFD + ArcFace), Selfies/Panoramas + collection framework, Settings AI/Storage. **Open:** places, video frame thumbs, perf baselines, MobileNetV4 tags / captioning later |
| **3** | Future | Advanced / non-destructive editing history, plugins, optional E2E encrypted sync, mobile, AI memories/stories, dark/system theme, auto-updates |

### Deferred settings (not shown until shipped)

Settings only surfaces controls that already work. The placeholders below belong
in later scopes — do **not** put disabled “Soon” toggles back in the UI.

**Still Phase 2 (add Settings toggles when the feature lands):**

| Setting | Why it waits |
| --- | --- |
| Object detection / auto-tags | Needs MobileNetV4 (or similar) classifier |
| Auto albums / auto-tag on import | Needs content classifiers + album rules |
| AI device: GPU / CoreML EP picker | Runtime already targets CoreML when available; explicit picker later |
| Background: only when idle | Needs idle detection on the job queue |
| Vault auto-lock timer | Needs session idle tracking |
| Folder ignore patterns | Needs indexer path filters |
| Periodic auto-scan (5m / 30m) | Live `notify` watching covers the common case; periodic rescan is optional polish |
| Honour CPU profile / battery pause / cache budget | Prefs persist today; wire into indexer + embedder throttling |
| Export resize + naming schemes | Needs export pipeline options beyond ZIP-as-is |
| Honour import skip-duplicates / JPEG quality / strip-metadata | Prefs persist; apply in import + export paths |
| Restore previous session / reveal imports | Prefs persist; apply in shell startup + import completion |

**Phase 3+ (product / packaging):**

| Setting | Why later |
| --- | --- |
| Dark / system theme | Spec stays light-only through Phase 2 |
| Accent / blur / theming kits | Brand is fixed; avoid palette churn mid-phase |
| Launch at login | Needs installer / OS login-item integration |
| Show hidden files | Niche; pairs with ignore-pattern work |
| Custom keyboard shortcuts | Needs a binding editor + conflict UI |
| Pause while gaming / manual worker count | Power-user OS integrations |
| In-app update check / background download | Packaging + signed release channel |
| Localisation beyond English | String extraction + translation workflow |

---

## Open questions

Resolved:

- Primary user = archive/pro organisers (privacy-first consumers now secondary).
- Package manager = Bun.
- MVP scale target = 1M assets.
- Crash/diagnostics = local logs only.
- EXIF stack = `kamadak-exif`.
- Thumbnails = JPEG max 320px, no eviction; video thumbs are placeholders.
- Minimum OS = macOS 13+, Windows 10+, recent glibc Linux.
- Product name = **LUMORA** (bundle id unchanged to preserve existing app data).
- Encrypted vault = delivered.

Locked for Phase 2:

- **ML runtime = ONNX Runtime via the `ort` crate.** Inference only. CoreML
  execution provider on macOS, CPU fallback elsewhere. Accepted cost: a native
  dependency in the build.
- **Model distribution = explicit, user-initiated download**, verified against a
  pinned SHA256 before use. This is the one approved network exception; it is
  never automatic, never silent, and never triggered by indexing or search.
- **First capability = semantic search (CLIP-class).** Determines the first
  model and the first Phase 2 migration.
- **Non-ML smart collections ship alongside** the collection framework refactor,
  so the framework is proven by real collections before ML lands.

Still open (not blocking the next slices):

1. Vector index strategy: brute force first; add ANN only when measurement demands it.
2. Offline reverse geocoding data source and licence for Places.
3. Video frame-extract strategy (may require ffmpeg).
4. Basic raster editing (rotate / crop / exposure) → **delivered** (destructive replace or copy; no edit history yet).
5. Dark / system theme (still light only).
6. Automated CI (still optional).

---

## Verification (spec gate)

- [x] Six core areas covered (objective, commands, structure, style, testing, boundaries)
- [x] Success criteria specific and testable
- [x] Always / Ask first / Never defined
- [x] Saved as `SPEC.md` in project root
- [x] Phase 1 recorded as delivered; vault reconciled as delivered early
- [x] Phase 2 blocking decisions locked (runtime = `ort`, user-initiated model
      download + SHA256, first capability = CLIP semantic search)
- [x] Semantic search + Settings/Developer IA + recent searches reflected as shipped
- [x] OCR (RapidOCR PP-OCRv4) + Documents/Receipts + `asset_text` reflected as shipped
- [x] Faces/People (InsightFace buffalo_l) + `faces`/`people` + People view reflected as shipped
- [ ] Human review and approval of remaining Phase 2 scope (places)

**Next step:** MobileNetV4 auto-tags; captioning later (Florence-2). Record Phase 1
perf baselines in `docs/perf-smoke.md`. Do not expand into Phase 3 without an
explicit SPEC update.
