# LUMORA

**Your memories, your machine.**

Local-first desktop photo & video library for large personal archives. Originals stay on your disk; metadata, thumbnails, and optional on-device AI never leave your computer. No cloud account, no telemetry, no hosted photo APIs.

| | |
| --- | --- |
| **Version** | 0.1.0 |
| **Platforms** | macOS first · Windows & Linux planned |
| **Stack** | [Tauri v2](https://v2.tauri.app/) · React · TypeScript · Rust · SQLite + FTS5 · ONNX Runtime |
| **License** | [MIT](./LICENSE) |
| **Docs** | [User guide](./guide.html) · [Landing](./index.html) · [Contributing](./CONTRIBUTING.md) |

Repository: [github.com/Anurag-error404/lumora](https://github.com/Anurag-error404/lumora)

---

## Why LUMORA

- **Local by design** — browse, search, and organise offline; models download only when you ask
- **Built for scale** — virtualised grid, incremental indexer, watched folders
- **Optional intelligence** — CLIP, OCR, faces, auto-tags, and captions run on-device via ONNX
- **Safe cleanup** — exact vs near duplicates, blurry review, soft trash with undo
- **Private vault** — encrypt albums and folders with Argon2id + XChaCha20-Poly1305

---

## Features

### Library & organisation

- Import folders recursively; originals are never moved or rewritten by indexing
- Watched folders for live add / change / remove
- Photos and videos in one library (video frame thumbs via system **ffmpeg** when available; soft-fail placeholder otherwise)
- Albums, tags, favourites, 1–5 ratings, colour labels
- Timeline by year / month; recently added and recently viewed
- Smart collections: Videos, RAW, Screenshots, Selfies, Panoramas, Documents, Receipts

### Search

- Full-text search (filename, tags, camera, OCR, people, auto-tags, captions)
- Filters such as `camera:iphone`, `rating>3`, `before:2024-01-01`
- Natural-language semantic search when CLIP is installed
- Recent search history in the toolbar and Discover

### On-device AI (optional)

Install from **Settings → AI**. The app works fully with zero models.

| Capability | Default backend | Notes |
| --- | --- | --- |
| Semantic search | CLIP ViT-B/32 | MIT |
| OCR | RapidOCR PP-OCRv4 | Apache-2.0 · powers Documents / Receipts |
| Faces / People | InsightFace buffalo_l | Non-commercial research · ≥80% front-facing faces kept |
| Auto-tags | MobileNetV4 | Apache-2.0 · shown in info panel + searchable |
| Image captions | Florence-2 Base | MIT · optional on-device captions + searchable |

Switch backends in the **model library**, then re-run processing if needed. Derived data (embeddings, faces, OCR, labels) is clearable without touching originals.

### Cleanup

- **Exact duplicates** — same SHA-256; bulk “keep one per group”
- **Near duplicates** — perceptual aHash, Hamming ≤ 2; review per group (no bulk delete)
- **Blurry images** — Laplacian variance scoring; trash after review
- Soft trash with retention · undo / redo

### Places & vault

- **Places** — GPS EXIF + offline reverse geocode (bundled GeoNames) and an offline geometry map (Natural Earth land + pins; no tile network)
- **Locked folder** — encrypted vault with recovery code

---

## Quick start

### Prerequisites

- [Bun](https://bun.sh)
- [Rust](https://www.rust-lang.org/tools/install) (stable) — ensure `~/.cargo/bin` is on `PATH`
- macOS: Xcode Command Line Tools
- Optional: [`ffmpeg`](https://ffmpeg.org/) / `ffprobe` on `PATH` for video thumbnails

### Run

```bash
bun install
bun run tauri dev      # desktop app + Vite (http://localhost:1420)
bun run tauri build    # release bundle
```

First launch: **Home → Import photos** (or drop a folder). Add **Watched folders** if you want new files indexed automatically.

Open [`guide.html`](./guide.html) for a full walkthrough (AI setup, duplicates, vault, shortcuts).

---

## Development

```bash
bun run typecheck          # TypeScript
bun test                   # frontend tests
cd src-tauri && cargo test # Rust unit tests
```

Performance smoke (ignored by default — needs a real run to claim scale):

```bash
cargo test --manifest-path src-tauri/Cargo.toml perf_smoke -- --ignored --nocapture
```

Targets and recorded runs: [`docs/perf-smoke.md`](./docs/perf-smoke.md).

### Project layout

```
src/                 React + TypeScript UI
src-tauri/           Rust backend (indexer, ML workers, vault, SQLite)
  migrations/        Schema migrations
  resources/         Bundled non-model assets (e.g. ImageNet labels)
docs/                ADRs, perf notes, third-party licence notes
index.html           Product landing (static)
guide.html           End-user guide (static)
app.html             Tauri webview entry
```

### Documentation map

| Doc | Audience |
| --- | --- |
| [guide.html](./guide.html) | End users |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | Contributors |
| [docs/README.md](./docs/README.md) | Docs index |
| [docs/adr/](./docs/adr/) | Architecture decisions |
| [docs/THIRD_PARTY.md](./docs/THIRD_PARTY.md) | Optional model / data licences |
| [docs/perf-smoke.md](./docs/perf-smoke.md) | Performance checklist |

---

## Privacy

- Core features work with **no network**
- Model downloads are **user-initiated** and SHA-256 verified
- Optional **in-app updates** check GitHub Releases when enabled (Settings → Updates)
- Logs and import analytics stay in local app data — never uploaded
- No analytics, crash upload, or cloud photo APIs

---

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| Click | Open media viewer |
| Checkbox / ⌘·Ctrl-click | Toggle selection |
| Space | Toggle / close viewer |
| ← / → | Previous / next |
| F | Favourite |
| 0–5 | Rate |
| ⌘A / Ctrl+A | Select all visible |
| Delete | Soft-delete (restore from Trash) |
| ⌘Z / Ctrl+Z | Undo |
| ⌘⇧Z / Ctrl+Y | Redo |
| Esc | Close viewer / clear selection |

More detail: [guide → Shortcuts](./guide.html#shortcuts).

---

## Contributing

PRs welcome. Please read [`CONTRIBUTING.md`](./CONTRIBUTING.md) — especially the privacy constraints (no telemetry, no cloud inference for library content).

Hard-to-reverse technical choices belong in a new ADR under `docs/adr/`.

---

## Support

If LUMORA helps you, you can support the project on [Ko-fi](https://ko-fi.com/anuragerror404). Support is optional and never changes privacy defaults or unlocks features. There’s also a short maker note in the app under **Settings → From the maker**.

---

## License

[MIT](./LICENSE) © 2026 Anurag Verma.

Optional AI models you download later keep their **upstream** licences (shown in Settings → AI). InsightFace face packs are typically **non-commercial research** only — see [`docs/THIRD_PARTY.md`](./docs/THIRD_PARTY.md).
