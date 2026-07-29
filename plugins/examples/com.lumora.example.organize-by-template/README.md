# Organize by Album Template

Moves selected photos into a template-driven folder structure on disk.

## Default template

```
${year}/${album}/${filename}
```

Example: `2024/Vacation Rome/IMG_0042.jpg`

## Template variables

| Variable | Description |
| --- | --- |
| `${year}` | 4-digit capture year |
| `${month}` | 2-digit capture month |
| `${day}` | 2-digit capture day |
| `${filename}` | Original filename including extension |
| `${album}` | Primary album name, or `Unsorted` if none |

## Permissions

- `read:assets` — read asset ids and paths
- `read:metadata` — read capture date and album membership
- `move:filesystem` — move files on disk (host validates paths, handles collisions)

## Preview / Apply flow

1. Preview shows a full plan: where each file will land, collisions, and skipped assets.
2. After confirmation, the host executes the moves transactionally.
3. The entire operation is undoable via **Edit → Undo**.

## Customising the template

Edit `OPTIONS.template` in `main.js` to change the folder pattern. Restart or re-enable the plugin for changes to take effect.
