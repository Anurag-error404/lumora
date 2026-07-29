# Lumora Plugin Author Guide

This guide explains how to build, customize, and run plugins in Lumora (Master Photo Manager).

## Overview

Plugins are small JavaScript folders installed under `{app_data}/plugins/`. Each plugin exposes one or more **selection actions** — batch operations run from the **Plugins** menu when photos are selected.

Properties of the plugin system:

- **No network access** — `fetch`, `XMLHttpRequest`, and external imports are blocked
- **Explicit permissions** — the host enforces declared capabilities in `lumora.plugin.json`
- **Inspectable** — all code and manifests live on disk
- **Auto-inferred permissions** — when you save in the Lumora editor, permissions are derived from your `main.js` source

For the full specification, see [plugins.md](./plugins.md) and [ADR 0002](./adr/0002-plugin-host.md).

---

## Quick start

1. Open **Plugins** in Lumora → **Create new plugin** (or **Documentation** for this guide on the web)
2. Fill in name, id, and action label
3. Edit `runAction` in the built-in editor
4. Save — permissions are inferred automatically
5. Select photos → **Plugins** in the selection bar → run your action

---

## Folder structure

```
{app_data}/plugins/com.yourname.my-plugin/
  lumora.plugin.json   ← manifest (required)
  main.js              ← entry script (required)
  README.md            ← optional documentation
```

The folder name **must match** the `id` field in the manifest.

---

## Manifest (`lumora.plugin.json`)

| Field | Required | Description |
|-------|----------|-------------|
| `id` | yes | Reverse-DNS identifier, e.g. `com.personal.my-rename` |
| `name` | yes | Display name |
| `version` | yes | Semver string |
| `apiVersion` | yes | Must be `1` |
| `description` | yes | Short summary |
| `author` | yes | Author name |
| `permissions` | yes | Array of permission tokens (auto-filled by editor) |
| `contributions.actions` | yes | Menu actions this plugin provides |
| `main` | no | Entry file, default `main.js` |

### Action contribution

```json
{
  "id": "rename-by-date",
  "label": "Rename by date…",
  "scope": "selection",
  "minSelection": 1,
  "maxSelection": 500
}
```

---

## Entry script (`main.js`)

Export a single async function:

```javascript
export async function runAction(actionId, context) {
  lumora.log("info", `${context.assetIds.length} selected`);
  context.reportProgress(0, context.assetIds.length);

  // Your logic here

  return { ok: true, message: "Done" };
}
```

### Context object

| Property | Type | Description |
|----------|------|-------------|
| `assetIds` | `string[]` | Selected photo ids |
| `mode` | `"preview"` \| `"apply"` | Run mode |
| `reportProgress(current, total)` | function | Updates the progress dialog |

### Return value

```javascript
{ ok: true, message: "Optional summary" }
// or
{ ok: false, message: "What went wrong" }
```

---

## `lumora.*` API

| API | Permission required |
|-----|---------------------|
| `lumora.log(level, message)` | none |
| `lumora.getAssets(ids)` | `read:assets` |
| Reading `.capturedAt`, `.rating`, `.camera`, etc. on assets | `read:metadata` |
| `lumora.setRating(id, rating)` | `write:metadata` |
| `lumora.setTags(id, tags)` | `write:metadata` |
| `lumora.renameAsset(id, newName)` | `rename:filesystem` |
| `lumora.moveAssets(ids, destDir)` | `move:filesystem` |
| `lumora.exportAssets(ids, options)` | `export:assets` |

The Lumora editor scans your source and adds the matching permissions to the manifest on save. You do **not** need to pick permissions manually.

---

## Permissions reference

| Token | Grants |
|-------|--------|
| `read:assets` | Load asset paths and ids |
| `read:metadata` | Read EXIF, ratings, camera info |
| `write:metadata` | Update ratings and tags |
| `rename:filesystem` | Rename files on disk |
| `move:filesystem` | Move files into folders |
| `copy:filesystem` | Copy files (reserved) |
| `delete:filesystem` | Delete files (reserved) |
| `export:assets` | Export resized copies |

Undeclared API calls fail at runtime with a permission error.

---

## Personal forks

To customize an existing plugin without modifying the original:

1. **Discover** or **Installed** → click **Customize** or **Save copy**
2. Lumora copies the plugin to a new id (e.g. `com.personal.fork-rename-by-date`)
3. Edit in the built-in editor and save

Forked plugins are marked in `README.md` with `Forked from: <original-id>`.

---

## Running plugins

1. Select one or more photos in the library
2. Click **Plugins** in the selection bar (bottom)
3. Choose your action
4. Watch progress in the run dialog; view history on the **Installed** tab

---

## Validation rules

The editor checks your plugin before save:

| Check | Severity |
|-------|----------|
| `runAction` function present | Error |
| No `fetch` / `XMLHttpRequest` | Error |
| `export async function runAction` recommended | Warning |
| External `import` / `require` | Warning |

---

## Example plugins

First-party examples ship in `plugins/examples/`:

- `hello-selection` — logging only, no permissions
- `rename-by-date` — rename with metadata
- `export-web-ready` — export resized copies
- `organize-by-template` — move into folder structure

Install from **Plugins → Discover**, or customize any example into a personal fork.

---

## Troubleshooting

| Symptom | Likely cause |
|---------|--------------|
| Action not in menu | Plugin disabled or not installed |
| Permission error at runtime | API used but not declared — re-save in editor |
| `PLUGIN_RUNTIME_ERROR` | JS exception in `runAction`; check run history |
| Timeout after 120s | Infinite loop or unresolved promise |

---

## Further reading

- [plugins.md](./plugins.md) — full specification
- [plugin.schema.json](./plugin.schema.json) — JSON schema for manifests
- [ADR 0002: Plugin Host](./adr/0002-plugin-host.md) — architecture decisions
