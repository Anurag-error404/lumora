# Export — Web Ready

Exports selected photos as web-optimised JPEG (max 2048 px long edge, quality 85) packaged in a ZIP archive.

## Permissions

- `read:assets` — read asset ids and paths
- `export:assets` — write files to a user-picked destination via the host export pipeline

## Options (hard-coded in v1)

| Option | Value |
| --- | --- |
| JPEG quality | 85 |
| Max long edge | 2048 px |
| Metadata | stripped |
| Folder structure | flat (original filenames) |

## Behaviour

- Vault-locked assets are silently skipped with a log warning.
- The host opens a save-panel for the user to choose the destination ZIP path.
- A preview is shown first listing how many photos will be exported.
