# Rename by Capture Date

Batch-renames selected photos to `YYYY-MM-DD_<shortid>.<ext>` using the EXIF capture date.

## Permissions

- `read:assets` — access asset paths and ids
- `read:metadata` — read capture date from EXIF
- `rename:filesystem` — rename files on disk (host validates paths, no `..` allowed)

## Behaviour

| Situation | What happens |
| --- | --- |
| Asset has EXIF `capturedAt` | Renamed to `2024-07-15_a1b2c3d4.jpg` |
| Asset has no date | Skipped with a warning in the log |
| Vault-locked asset | Skipped (plugins are read-only for vault assets) |

## Preview / Apply flow

1. The host runs the action in **preview** mode first.
2. A plan is shown listing which files will be renamed and which skipped.
3. After the user confirms, the host calls **apply** — actual renames happen.
4. Every rename is recorded in the undo history; use **Edit → Undo** to reverse.
