# LUMORA

Local-first photo & video library — **your memories your machine.**

Built with Tauri v2 + React + TypeScript + Rust.

See `SPEC.md` for Phase 1 scope, `prd.md` for full vision, and `tasks/` for the implementation plan.

## Prerequisites

- [Bun](https://bun.sh)
- [Rust](https://www.rust-lang.org/tools/install) (stable) — ensure `~/.cargo/bin` is on `PATH`
- macOS: Xcode Command Line Tools (or Xcode)

## Commands

```bash
bun install
bun run tauri dev      # desktop app + Vite
bun run tauri build    # release bundle
bun run typecheck
bun test
cd src-tauri && cargo test
```

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| Click | Open media viewer (preview) |
| Checkbox / ⌘·Ctrl click | Toggle selection |
| ⌥·Alt click | Select only this item |
| Drag on grid | Marquee multi-select |
| ★ / heart on thumbnail | Toggle favourite |
| F | Favourite / unfavourite (selection or open viewer item) |
| Space | Toggle / close media viewer |
| ← / → (or ↑ / ↓) | Previous / next in viewer |
| 0–5 | Rate selection (or open viewer item) |
| ⌘A / Ctrl+A | Select all visible |
| Delete / Backspace | Soft-delete (or restore in Trash) |
| ⌘Z / Ctrl+Z | Undo |
| ⌘⇧Z / Ctrl+Y | Redo |
| Esc | Close viewer / clear selection |

While adding photos to an album, plain click selects instead of opening the viewer.

Selection toolbar: favourite, tag, album, rating, colour label, export ZIP, trash.

## Stack

Scaffolded with [`create-tauri-app`](https://v2.tauri.app/start/create-project/) (`react-ts`, manager `bun`, Tauri v2). Light theme only in Phase 1.
