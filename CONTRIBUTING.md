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

## Commit messages

Write commits so history stays readable and reviewable. Prefer [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>: <short summary in imperative mood>

[optional body — why the change exists, not a restatement of the diff]
```

**Rules**

- Subject ≤ ~72 characters; imperative mood (`add`, `fix`, `remove` — not `added` / `fixes`).
- Explain **why**, not what the diff already shows.
- One logical change per commit; don’t mix features, refactors, and drive-by formatting.
- Use a type prefix:

| Type | Use for |
| --- | --- |
| `feat` | New user-facing behaviour |
| `fix` | Bug fix |
| `refactor` | Internal change with no behaviour change |
| `test` | Tests only |
| `docs` | Docs / guide / ADR only |
| `chore` | Tooling, CI, deps, config |
| `perf` | Performance improvement |

**Examples**

```text
feat: retry embedding pipeline on cold start

fix: restore OCR hits in image search results

docs: document updater signing secrets for releases
```

Avoid vague subjects (`update`, `fix stuff`, `wip`) and giant catch-all commits.

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

