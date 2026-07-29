# ADR 0002: Plugin host architecture (Phase 3)

## Status

Proposed

## Context

Phase 3 lists **plugins** as the next major slice after Memories, ANN, and auto-updates. The product needs user extensions without:

- Network calls at runtime
- Unsandboxed native code in the hot path
- Forking the app for small batch workflows

Earlier SPEC non-goals mentioned a “full professional RAW editing or plugin system” — that referred to Lightroom-class develop modules, not small local automation.

## Decision

1. **Folder-based installs** under `{app_data}/plugins/<id>/` with a `lumora.plugin.json` manifest.
2. **JavaScript plugins** executed in a **QuickJS** sandbox (`rquickjs`), not native dylibs or in-process WASM for v1.
3. **Allowlisted host API** (`lumora.*`) with manifest permission tokens; Rust performs all SQLite and filesystem I/O.
4. **v1 contribution type:** `selection` actions only (user-initiated, max 500 assets).
5. **Settings → Extensions** for install / enable / remove; no marketplace in v1.

## Alternatives considered

| Option | Rejected because |
| --- | --- |
| Native `.dylib` plugins | No sandbox; ABI/version pain across OS updates |
| WASM-only | Harder author ergonomics for metadata scripts |
| Declarative YAML recipes only | Too limited for real-world rename/export logic |
| WebView iframe plugins | DOM access + larger attack surface; awkward FS |
| Tauri sidecar processes | Heavy; IPC overhead for simple batch ops |

## Consequences

- New dependency: `rquickjs` (evaluate licence + binary size in Milestone 2 spike).
- Plugin authors debug via local logs and Developer page, not Chrome DevTools (unless we add a dev bridge later).
- Core ML, vault crypto, and indexer internals stay closed — plugins compose public operations only.
- Full spec and task breakdown: [`../plugins.md`](../plugins.md).
