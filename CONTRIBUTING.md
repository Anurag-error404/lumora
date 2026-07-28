# Contributing to LUMORA

Thanks for helping. LUMORA is a local-first photo library — keep changes aligned with **privacy by default** (no telemetry, no cloud photo APIs, models only on explicit user download).

## Before you start

1. Read the [user guide](./guide.html) to understand the product surface.
2. Skim [`docs/adr/`](./docs/adr/) for locked technical choices.
3. Prefer small, reviewable PRs over large catch-all branches.

## Setup

```bash
bun install
bun run tauri dev
```

Requirements: Bun, Rust stable, Xcode CLT (macOS). Optional: ffmpeg/ffprobe for video thumbs.

## Checks to run locally

```bash
bun run typecheck
bun test
cd src-tauri && cargo test
```

## Guidelines

- **Do not** add network calls for core indexing, search, or inference.
- **Do not** log photo paths or EXIF to remote services.
- Match existing UI language (light theme, Syne/Figtree, green accent) when touching the app.
- New irreversible decisions (crypto, ML runtime, schema strategy) → add an ADR under `docs/adr/`.
- User-facing behaviour changes → update [`guide.html`](./guide.html) (and the landing features list in [`index.html`](./index.html) when the pitch changes).
- Keep commits focused; avoid bundling unrelated refactors with feature work.

## Pull requests

- Describe **why** the change exists and how you tested it.
- Include screenshots or short clips for UI changes.
- Note any migration or model-download impact for existing libraries.

## Releasing / in-app updates

Tagged releases (`v*`) run [`.github/workflows/release.yml`](./.github/workflows/release.yml), which builds installers **and** signed updater artifacts (`*.sig` + `latest.json`).

Required repo secrets (Settings → Secrets and variables → Actions):

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Full contents of the private key from `bun run tauri signer generate` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Key password, or leave empty if the key has none |

The matching **public** key is embedded in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`). Losing the private key permanently breaks in-app updates for existing installs — back it up offline.

Local key material for this repo lives in `.tauri-keys/` (gitignored). Example:

```bash
gh secret set TAURI_SIGNING_PRIVATE_KEY < .tauri-keys/lumora.key
```

## License

By contributing, you agree your contributions are licensed under the same
[MIT License](./LICENSE) as the project. Do not contribute code you cannot
release under MIT. Optional model weights remain under their upstream terms
([`docs/THIRD_PARTY.md`](./docs/THIRD_PARTY.md)).

