# Lumora Plugins — MVP spec (Phase 3)

> **Status:** Implemented (Milestones 1–2). See [plugin-author-guide.md](./plugin-author-guide.md) for author documentation.  
> **Audience:** Contributors implementing Phase 3 extensions.  
> **Parent:** [`SPEC.md`](../SPEC.md) Phase 3 roadmap.

## Objective

Let users extend Lumora with **small, local, inspectable automation** — batch actions on selected photos, export helpers, and metadata utilities — **without** weakening the product’s local-first / no-network guarantees.

### What success looks like

1. A user drops a folder into `{app_data}/plugins/` and sees it in **Settings → Extensions**.
2. With photos selected, a plugin action appears in the selection toolbar (or Plugins submenu).
3. Running the action shows progress, respects permissions, and logs locally on failure.
4. Disabling or deleting a plugin folder removes its UI and stops all hooks immediately.
5. A first-party example plugin in the repo demonstrates the full flow end-to-end.

### Primary users

- Power users who want repeatable library workflows (rename patterns, sidecar sync, custom export naming).
- Contributors who want to ship small tools without forking the app.

### Non-goals (v1)

| Out of scope | Why |
| --- | --- |
| Plugin marketplace / auto-download | Network + trust model not ready |
| Arbitrary native code (`.dylib`, `.so`) | No sandbox; support burden |
| Deep RAW / develop-module hooks | Conflicts with “basic edit” scope |
| Replacing ONNX workers (CLIP, faces, OCR) | Core ML stays first-party |
| Background auto-run on every index | Surprise CPU + privacy risk |
| Network access at runtime | Breaks core product promise |
| UI theme / sidebar replacement | Fragile; not needed for MVP |

---

## Design principles

1. **Filesystem is the install surface** — same mental model as `models/`: visible, deletable, no hidden state.
2. **Deny by default** — manifest declares permissions; host enforces before every call.
3. **User-initiated** — v1 actions run only when the user explicitly chooses them (no silent hooks).
4. **Rust owns I/O** — plugins never touch SQLite or originals directly; they call an allowlisted host API.
5. **Versioned contract** — `apiVersion` in manifest; host rejects incompatible plugins with a clear error.
6. **No direct DB access** — SQLite handles and query primitives are never exposed to plugins.
7. **Immutable asset snapshots** — `getAssets()` returns read-only value objects; there is no mutation path for asset state in JS.
8. **Execution history is always written** — every plugin run produces a signed record (outcome, duration, asset count, log lines) stored locally. History is per-plugin, never aggregated across users, and the user can clear it at any time.

---

## Plugin Philosophy

Plugins exist for workflows that are:

- user-specific
- niche
- automation-oriented
- experimental

Core functionality that benefits most users should remain part of Lumora itself.

---

## Assumptions

1. **Execution model:** JavaScript plugins in a sandboxed QuickJS runtime (`rquickjs`), not native dylibs.
2. **Install path:** `{app_data}/plugins/<plugin-id>/`.
   - For normal users: the host **copies** the plugin folder into `plugins/`.
   - For contributors: Developer Mode may allow symlinks / reference paths (for fast iteration).
3. **One library per app instance** — plugins read the active library DB via the host; no multi-library plugins in v1.
4. The **plugin API is platform-neutral**. Development may begin on macOS, but host API behaviour should remain consistent across macOS, Windows, and Linux.
5. **English-only manifest strings** in v1; localisation deferred.

Correct these before implementation if any are wrong.

---

## Plugin package layout

```
{app_data}/plugins/
  com.lumora.example.rename-by-date/
    lumora.plugin.json      # manifest (required)
    main.js                 # entry module (required when contributions include "script")
    history.jsonl           # execution history (host-managed, append-only, trimmed to 200 records)
    README.md               # optional, shown in Settings
    icon.png                # optional, 64×64
```

**Plugin id rules:** reverse-DNS string (`com.author.name`), stable for the life of the install. Folder name must match `id`.

---

## Manifest (`lumora.plugin.json`)

Point your editor at the schema for autocomplete:

```json
{
  "$schema": "../../docs/plugin.schema.json",
  "id": "com.lumora.example.rename-by-date",
  "name": "Rename by capture date",
  "version": "1.0.0",
  "apiVersion": 1,
  "description": "Batch-rename selected files using EXIF capture date.",
  "author": "Lumora contributors",
  "permissions": [
    "read:assets",
    "read:metadata",
    "rename:filesystem"
  ],
  "contributions": {
    "actions": [
      {
        "id": "rename-by-date",
        "label": "Rename by capture date",
        "scope": "selection",
        "minSelection": 1,
        "maxSelection": 500
      }
    ]
  },
  "main": "main.js"
}
```

### Fields

| Field | Required | Notes |
| --- | --- | --- |
| `id` | yes | Unique reverse-DNS id |
| `name` | yes | Shown in UI |
| `version` | yes | Semver string |
| `apiVersion` | yes | Host API contract (start at `1`) |
| `permissions` | yes | May be empty `[]` for no-op demos |
| `contributions.actions` | v1 | Selection-scoped commands |
| `main` | yes if script | Relative path to JS entry |

### Permission tokens (v1)

| Permission | Grants |
| --- | --- |
| `read:assets` | Asset ids, paths, media type, thumbnail paths |
| `read:metadata` | EXIF fields, tags, rating, label, favourite, captions (read) |
| `write:metadata` | Tags, rating, label, favourite (not paths) |
| `rename:filesystem` | Rename files the library already tracks (host validates) |
| `move:filesystem` | Move files across directories / change relative paths (host validates) |
| `copy:filesystem` | Copy files into a new structure (host validates) |
| `delete:filesystem` | Delete files already tracked (host validates) |
| `export:assets` | Write files to a user-picked destination via host export helper |

**Vault note (v1):** plugins are **read-only** for vault-encrypted assets. Methods that modify disk paths or metadata (rename/move/copy/delete, `setTags`, `setRating`) throw when the selected asset is vault-locked/encrypted.

**Not grantable in v1:** raw SQL, network, spawn processes, read arbitrary paths, vault decrypt, ML inference hooks.

---

## API Stability

Lumora's plugin API follows semantic versioning.

- Minor releases may add new host APIs.
- Existing APIs will remain backwards compatible within the same `apiVersion`.
- Deprecated APIs will remain supported for at least one major host release.
- Breaking changes require incrementing `apiVersion`.

---

## Plugin API Compatibility Matrix

The host publishes which plugin API versions it supports.

| Lumora host version line | Supported plugin API |
| --- | --- |
| Current host releases (this repo) | v1 (`apiVersion: 1`) |

---

## JSON Schema for Manifest

An official JSON Schema is provided at `docs/plugin.schema.json` to enable:

- editor autocomplete
- manifest validation in CI
- consistent contributor UX

---

## Future Extension Points (reserved names)

In v1, only `contributions.actions` are supported.

However, the following names are reserved to avoid breaking renames later:

- `events`
- `panels`
- `providers`
- `views`

These may be added in future plugin API versions; unknown contribution keys are ignored or rejected depending on host strictness.

---

## Host API (JavaScript, `apiVersion: 1`)

Plugins receive a frozen `lumora` global. All methods return Promises. See the first-party examples further below for full working implementations of every pattern.

### `context` (action invocation)

```ts
type ActionContext = {
  actionId: string;
  assetIds: string[];       // current selection, pre-validated against min/max
  libraryId: string;        // constant "default" in v1
  mode: "preview" | "apply"; // destructive actions run preview first, then apply after confirmation
  // Reserve for long-running progress reporting (exact host UI integration can evolve).
  // Host calls are throttled; plugins should treat it as "best effort".
  reportProgress: (current: number, total: number) => void;
};
```

### `lumora` surface (v1)

| Method | Permission | Description |
| --- | --- | --- |
| `getAssets(ids: string[])` | `read:assets` (+ `read:metadata` for meta fields) | Summaries for selected ids (includes `vaultLocked` when applicable) |
| `renameAsset(id, newFileName)` | `rename:filesystem` | Destructive: requires preview (`context.mode === "preview"`) then apply; preview validates/stages only |
| `setTags(id, tags: string[])` | `write:metadata` | Destructive: requires preview then apply (preview validates/stages only) |
| `setRating(id, rating: number \| null)` | `write:metadata` | Destructive: requires preview then apply (preview validates/stages only) |
| `moveAssets(assetIds, destinationFolder)` | `move:filesystem` | Destructive: requires preview then apply; host performs safe moves (collisions, undo/history) |
| `copyAssets(assetIds, destinationFolder)` | `copy:filesystem` | Requires preview then apply; host performs safe copies and records history for revert (delete newly created copies) |
| `createFolder(path)` | `move:filesystem` | Requires preview then apply; host validates destination root and creates missing directories as part of the apply run |
| `moveAlbumToFilesystem(albumId, options)` | `move:filesystem` | Destructive: requires preview then apply; host reorganizes the album’s assets safely (collisions, undo/history) |
| `organizeAssets(options)` | `move:filesystem` / `copy:filesystem` | Template-driven disk reorganisation (e.g. `${year}/${album}/${filename}`); preview returns a plan + collision info, apply executes transactionally and supports Undo |
| `exportAssets(ids, options)` | `export:assets` | Opens save dialog; reuses host ZIP/export pipeline |
| `log(level, message)` | none | Writes to local plugin log (Developer page) |

Errors throw `LumoraError` with `code` + `message`; host surfaces them in the UI toast.

---

## Plugin Execution History

### Purpose

Every plugin run produces a structured record the user can inspect at any time. This serves three goals:

1. **Transparency** — the user can always see what a plugin did, to which assets, when, and whether it succeeded.
2. **Accountability** — if something in the library looks wrong, the history file is the first place to look.
3. **Debuggability** — developers can read the log lines without opening the Developer page.

### Data model

```ts
type PluginRunRecord = {
  runId: string;            // UUID
  pluginId: string;         // reverse-DNS manifest id
  pluginVersion: string;    // version from manifest at run time
  actionId: string;
  startedAt: string;        // ISO 8601
  finishedAt: string;       // ISO 8601
  durationMs: number;
  mode: "preview" | "apply";
  outcome: "ok" | "cancelled" | "timeout" | "error";
  errorCode?: string;       // LumoraError.code when outcome === "error"
  errorMessage?: string;
  assetsRequested: number;  // assetIds.length passed in
  assetsAffected: number;   // assets for which a write actually occurred
  assetsSkipped: number;    // vault-locked, no-date, etc.
  logLines: PluginLogLine[];
};

type PluginLogLine = {
  level: "info" | "warn" | "error";
  message: string;
  timestampMs: number;      // ms offset from startedAt
};
```

### Storage

History is written to `{app_data}/plugins/<plugin-id>/history.jsonl` — one JSON object per line (JSONL). The host writes after every run regardless of outcome.

```
{app_data}/plugins/
  com.lumora.example.rename-by-date/
    lumora.plugin.json
    main.js
    history.jsonl           ← append-only; host writes this
    README.md
    icon.png
```

**Retention:** the host trims `history.jsonl` to the last **200 run records** on each write to prevent unbounded growth. Users can clear history manually from Settings → Extensions or Developer page.

### Host API — `lumora.getHistory()`

Plugins may call `lumora.getHistory()` from within a run to inspect their own past records. This allows plugins to make informed decisions (e.g. detect a recent failed run before proceeding). Accessing another plugin's history is not permitted.

```ts
// Returns the most recent records for this plugin, newest first.
lumora.getHistory(options?: { limit?: number }): Promise<PluginRunRecord[]>
```

| Method | Permission | Description |
| --- | --- | --- |
| `getHistory(options?)` | none | Last N run records for this plugin (default 20, max 100). Read-only; returns immutable records. |

### UI integration

| Surface | Behaviour |
| --- | --- |
| **Settings → Extensions** | Each plugin card shows last run date + outcome badge (`✓ OK`, `✗ Error`, `↩ Cancelled`). Expandable history list shows last 20 runs with duration, assets affected, and log lines. "Clear history" button per plugin. |
| **Developer page** | Full history tail (most recent 50 runs across all plugins) with log lines. "Clear all plugin history" action. |

---

## Standard Error Codes

`LumoraError.code` is one of:

- `PLUGIN_TIMEOUT`
- `PLUGIN_CANCELLED`
- `PLUGIN_PERMISSION_DENIED`
- `PLUGIN_API_MISMATCH`
- `PLUGIN_INVALID_MANIFEST`
- `PLUGIN_RUNTIME_ERROR`
- `PLUGIN_NOT_FOUND`

### Runtime limits

| Limit | Value |
| --- | --- |
| JS heap | 64 MB per run |
| Wall time | 120 s (user can cancel) |
| Max selection | 500 assets per action (manifest can lower) |
| Concurrent runs | 1 per plugin |
| Runtime isolation | Fresh JS runtime per action run (no persisted globals) |

---

## UI integration (v1)

| Surface | Behaviour |
| --- | --- |
| **Settings → Extensions** | List installed plugins; enable/disable; open folder; view permissions; “Add plugin folder…” |
| **Selection toolbar** | `Plugins ▾` submenu when ≥1 enabled plugin contributes a `selection` action |
| **Progress** | Reuse existing `setBusy` / toast patterns; long runs show count `n / total` (and may also reflect `context.reportProgress`). |
| **Developer page** | Last plugin error, log tail, loaded script path; history tail across all plugins (last 50 runs); "Clear all plugin history" action. |

**Settings honesty:** panel hidden until at least the registry + one action path works (same rule as other Settings).

### Permission dialog UX

When the user enables or runs a plugin with declared permissions, the host presents a permission list in a simple checkbox-style layout.

Example:

1. Organize by Album Template
2. Permissions
   - ✓ Read Assets
   - ✓ Read Metadata
   - ✓ Move Files
   - ✗ No Network Access
   - ✗ No Database Access

---

## Rust architecture

```
src-tauri/src/plugins/
  mod.rs           # public API: scan, registry, run_action
  manifest.rs      # parse + validate lumora.plugin.json
  registry.rs      # enabled state in preferences.json
  host.rs          # rquickjs runtime + lumora bindings
  permissions.rs   # token checks
  history.rs       # read/write/trim history.jsonl per plugin
```

### Tauri commands (proposed)

| Command | Purpose |
| --- | --- |
| `list_plugins` | Installed + enabled + contributions |
| `set_plugin_enabled` | Toggle without deleting files |
| `install_plugin_dir` | Validate folder + copy/symlink into `plugins/` |
| `remove_plugin` | Delete plugin folder + registry entry |
| `run_plugin_action` | `pluginId`, `actionId`, `assetIds[]` |
| `get_plugin_history` | `pluginId`, `limit?` → `PluginRunRecord[]` |
| `clear_plugin_history` | `pluginId` → deletes `history.jsonl` for that plugin |
| `clear_all_plugin_history` | Deletes all `history.jsonl` files across installed plugins |

### Preferences extension

```json
{
  "plugins": {
    "enabled": {
      "com.lumora.example.rename-by-date": true
    }
  }
}
```

### AppPaths extension

```rust
pub plugins_dir: PathBuf,  // app_data.join("plugins")
```

Created alongside `models_dir` in `AppPaths::from_app_data`.

---

## Security model

```
User clicks action
  → host checks plugin enabled + manifest valid
  → host checks selection bounds
  → host opens a PluginRunRecord (startedAt = now, outcome = pending)
  → host loads JS in a fresh QuickJS context (fresh runtime per action)
  → host runs destructive actions in two phases:
      preview (no changes) → user confirmation → apply (changes)
  → each lumora.* call re-checks permission token
  → operations go through existing rename/export/edit modules (path canonicalisation, no `..`)
  → transactions are per-asset with progress reporting
  → on success: library refresh + one grouped history entry (supports Undo)
  → on failure: no partial changes for the affected asset (other assets may continue)
  → host finalises PluginRunRecord (finishedAt, outcome, assetsAffected, logLines)
  → host appends record to {plugin_dir}/history.jsonl
  → host trims history.jsonl to last 200 records
```

**Trust model:** v1 treats all plugins as **user-trusted** (user installed the folder). Code signing / notarisation of plugins is out of scope.

---

## First-party examples (ship in repo)

Four plugins live under `plugins/examples/` in the repository. Each is self-contained, uses the correct preview/apply pattern, calls `context.reportProgress`, and handles vault-encrypted assets safely.

Copy any example folder to `{app_data}/plugins/` to test it locally, or follow the Developer Mode instructions in `CONTRIBUTING.md`.

| Plugin | Demonstrates |
| --- | --- |
| `plugins/examples/com.lumora.example.hello-selection` | No permissions — logs count, tests lifecycle |
| `plugins/examples/com.lumora.example.rename-by-date` | `read:metadata` + `rename:filesystem` + preview + undo |
| `plugins/examples/com.lumora.example.export-web-ready` | `export:assets` with resize option, progress, vault-skip |
| `plugins/examples/com.lumora.example.organize-by-template` | template-driven disk organize with preview plan + confirm + undo |

---

### Example 1 — `com.lumora.example.hello-selection`

Demonstrates the minimal lifecycle: no permissions, no destructive writes, no preview required.

**Layout:**
```
plugins/examples/com.lumora.example.hello-selection/
  lumora.plugin.json
  main.js
  README.md
```

**`lumora.plugin.json`**
```json
{
  "$schema": "../../../docs/plugin.schema.json",
  "id": "com.lumora.example.hello-selection",
  "name": "Hello Selection",
  "version": "1.0.0",
  "apiVersion": 1,
  "description": "Logs the count and ids of selected assets. No modifications.",
  "author": "Lumora contributors",
  "permissions": [],
  "contributions": {
    "actions": [
      {
        "id": "hello-selection",
        "label": "Hello Selection",
        "scope": "selection",
        "minSelection": 1,
        "maxSelection": 500
      }
    ]
  },
  "main": "main.js"
}
```

**`main.js`**
```js
export async function runAction(actionId, context) {
  // This action has no side effects, so mode does not matter here.
  lumora.log("info", `hello-selection: ${context.assetIds.length} asset(s) selected`);
  context.reportProgress(0, context.assetIds.length);
  for (let i = 0; i < context.assetIds.length; i++) {
    lumora.log("info", `  [${i + 1}] ${context.assetIds[i]}`);
    context.reportProgress(i + 1, context.assetIds.length);
  }
  return { ok: true, message: `Logged ${context.assetIds.length} asset(s)` };
}
```

---

### Example 2 — `com.lumora.example.rename-by-date`

Demonstrates `rename:filesystem` with preview/apply, progress, vault-skip, and undo integration.

**Layout:**
```
plugins/examples/com.lumora.example.rename-by-date/
  lumora.plugin.json
  main.js
  README.md
```

**`lumora.plugin.json`**
```json
{
  "$schema": "../../../docs/plugin.schema.json",
  "id": "com.lumora.example.rename-by-date",
  "name": "Rename by Capture Date",
  "version": "1.0.0",
  "apiVersion": 1,
  "description": "Renames selected files to YYYY-MM-DD_<shortid>.<ext> using EXIF capture date.",
  "author": "Lumora contributors",
  "permissions": [
    "read:assets",
    "read:metadata",
    "rename:filesystem"
  ],
  "contributions": {
    "actions": [
      {
        "id": "rename-by-date",
        "label": "Rename by capture date…",
        "scope": "selection",
        "minSelection": 1,
        "maxSelection": 500
      }
    ]
  },
  "main": "main.js"
}
```

**`main.js`**
```js
// Build the new filename for an asset.
// Returns null when there is no date to work with.
function buildNewName(asset) {
  const date = asset.capturedAt ?? asset.createdAt;
  if (!date) return null;
  const ext = asset.path.split(".").pop() ?? "jpg";
  return `${date.slice(0, 10)}_${asset.id.slice(0, 8)}.${ext}`;
}

export async function runAction(actionId, context) {
  // getAssets() returns immutable snapshots — no mutation possible in JS.
  const assets = await lumora.getAssets(context.assetIds);
  const total = assets.length;

  // ── Preview phase ──────────────────────────────────────────────
  // In preview mode we validate + stage only; no disk or DB changes.
  if (context.mode === "preview") {
    const plan = [];
    for (const asset of assets) {
      if (asset.vaultLocked) {
        // Vault-encrypted assets are read-only for plugins.
        plan.push({ id: asset.id, skip: true, reason: "vault-locked" });
        continue;
      }
      const newName = buildNewName(asset);
      if (!newName) {
        plan.push({ id: asset.id, skip: true, reason: "no-date" });
        continue;
      }
      plan.push({ id: asset.id, newName, skip: false });
    }
    const willRename = plan.filter((p) => !p.skip).length;
    return {
      ok: true,
      previewPlan: plan,
      message: `Will rename ${willRename} of ${total} file(s).`,
    };
  }

  // ── Apply phase ────────────────────────────────────────────────
  // Host confirms with the user after preview; we reach here only on confirm.
  let renamed = 0;
  let skipped = 0;
  for (let i = 0; i < assets.length; i++) {
    const asset = assets[i];
    context.reportProgress(i, total);

    if (asset.vaultLocked) {
      lumora.log("warn", `skip vault-locked asset: ${asset.id}`);
      skipped++;
      continue;
    }
    const newName = buildNewName(asset);
    if (!newName) {
      lumora.log("warn", `skip no-date asset: ${asset.id}`);
      skipped++;
      continue;
    }
    // renameAsset is per-asset transactional — a failure here does not
    // affect assets already renamed.  The host records each rename in the
    // undo history so the user can revert the whole batch.
    await lumora.renameAsset(asset.id, newName);
    renamed++;
  }

  context.reportProgress(total, total);
  return {
    ok: true,
    message: `Renamed ${renamed} file(s), skipped ${skipped}.`,
  };
}
```

---

### Example 3 — `com.lumora.example.export-web-ready`

Demonstrates `export:assets` with a resize option, progress reporting, and vault-asset skipping.

**Layout:**
```
plugins/examples/com.lumora.example.export-web-ready/
  lumora.plugin.json
  main.js
  README.md
```

**`lumora.plugin.json`**
```json
{
  "$schema": "../../../docs/plugin.schema.json",
  "id": "com.lumora.example.export-web-ready",
  "name": "Export — Web Ready",
  "version": "1.0.0",
  "apiVersion": 1,
  "description": "Exports selected photos as web-optimised JPEG (max 2048 px, quality 85) to a ZIP archive.",
  "author": "Lumora contributors",
  "permissions": [
    "read:assets",
    "export:assets"
  ],
  "contributions": {
    "actions": [
      {
        "id": "export-web-ready",
        "label": "Export web-ready ZIP…",
        "scope": "selection",
        "minSelection": 1,
        "maxSelection": 500
      }
    ]
  },
  "main": "main.js"
}
```

**`main.js`**
```js
const EXPORT_OPTIONS = {
  stripMetadata: true,
  jpegQuality: 85,
  maxEdge: 2048,
  preserveFolderStructure: false,
  naming: "original",
};

export async function runAction(actionId, context) {
  const assets = await lumora.getAssets(context.assetIds);
  const total = assets.length;

  // ── Preview phase ──────────────────────────────────────────────
  // No file I/O in preview.  We just report what will be exported.
  if (context.mode === "preview") {
    const exportable = assets.filter((a) => !a.vaultLocked);
    const skipped = total - exportable.length;
    return {
      ok: true,
      message: `Will export ${exportable.length} photo(s) as web-ready JPEG${skipped > 0 ? ` (${skipped} vault-locked asset(s) skipped)` : ""}.`,
    };
  }

  // ── Apply phase ────────────────────────────────────────────────
  // Filter out vault-locked assets — plugins are read-only for these.
  const exportIds = [];
  for (const asset of assets) {
    if (asset.vaultLocked) {
      lumora.log("warn", `skip vault-locked asset: ${asset.id}`);
      continue;
    }
    exportIds.push(asset.id);
  }

  if (exportIds.length === 0) {
    return { ok: false, message: "All selected assets are vault-locked; nothing to export." };
  }

  context.reportProgress(0, exportIds.length);

  // exportAssets opens a save-panel for the user, then writes a ZIP.
  // Progress is reported inside the host as it processes each file;
  // we report start + done here to frame the operation.
  const result = await lumora.exportAssets(exportIds, EXPORT_OPTIONS);

  context.reportProgress(exportIds.length, exportIds.length);
  lumora.log("info", `export-web-ready: ${result.exportedCount} file(s) → ${result.destinationPath}`);

  return {
    ok: true,
    message: `Exported ${result.exportedCount} photo(s).`,
  };
}
```

---

### Example 4 — `com.lumora.example.organize-by-template`

Demonstrates template-driven disk reorganisation using the host API, with a **preview plan** first (no immediate execution), then an apply phase with Undo/history.

**Layout:**
```
plugins/examples/com.lumora.example.organize-by-template/
  lumora.plugin.json
  main.js
  README.md
```

**`lumora.plugin.json`**
```json
{
  "$schema": "../../../docs/plugin.schema.json",
  "id": "com.lumora.example.organize-by-template",
  "name": "Organize by Album Template",
  "version": "1.0.0",
  "apiVersion": 1,
  "description": "Moves selected photos into a template-based folder structure (e.g. ${year}/${album}/${filename}).",
  "author": "Lumora contributors",
  "permissions": [
    "read:assets",
    "read:metadata",
    "move:filesystem"
  ],
  "contributions": {
    "actions": [
      {
        "id": "organize-by-template",
        "label": "Organize by template…",
        "scope": "selection",
        "minSelection": 1,
        "maxSelection": 500
      }
    ]
  },
  "main": "main.js"
}
```

**`main.js`**
```js
// Template variables (resolved by the host):
// - ${year}, ${month}, ${day}
// - ${filename} (original file name)
// - ${album} (primary album name for the asset, if available)
//
// Never executes immediately: preview runs first and only returns a plan.
// Apply runs only after host confirmation, and supports Undo/history.
const OPTIONS = {
  strategy: "template",
  template: "${year}/${album}/${filename}",
  mode: "move",
};

export async function runAction(actionId, context) {
  const total = context.assetIds.length;

  if (context.mode === "preview") {
    // Host computes the full plan: collisions, skips, and final relative paths.
    // It must not touch disk/DB in preview mode.
    const preview = await lumora.organizeAssets({
      assetIds: context.assetIds,
      ...OPTIONS,
    });

    context.reportProgress(0, total);
    context.reportProgress(total, total);

    return {
      ok: true,
      message: preview?.message ??
        `Will reorganize ${total} asset(s) (preview mode).`,
      previewPlan: preview?.plan,
    };
  }

  // Apply phase: host executes the previously planned operation transactionally.
  const result = await lumora.organizeAssets({
    assetIds: context.assetIds,
    ...OPTIONS,
  });

  return {
    ok: true,
    message: result?.message ??
      `Organized selection (moved ${result?.movedCount ?? "?"}, skipped ${result?.skippedCount ?? "?"}).`,
  };
}
```

---

## Implementation plan

### Milestone 1 — Registry (no JS)

- [ ] `plugins/` directory + `AppPaths.plugins_dir`
- [ ] Manifest parser + validation (`apiVersion`, permissions, contributions)
- [ ] `list_plugins` / enable / disable / remove
- [ ] Settings → Extensions list UI (read-only metadata)
- [ ] Tests: valid/invalid manifests, duplicate ids, missing files

**Exit criteria:** User can install a folder and see it in Settings; no actions yet.

### Milestone 2 — Script host + execution history

- [ ] Add `rquickjs` dependency + `plugins/host.rs`
- [ ] Implement `lumora.getAssets`, `lumora.log`
- [ ] `run_plugin_action` with timeout + cancellation
- [ ] `plugins/history.rs`: write/read/trim `history.jsonl`; `get_plugin_history` + `clear_plugin_history` + `clear_all_plugin_history` commands
- [ ] `lumora.getHistory()` host binding
- [ ] Developer page: last error + log tail + history tail
- [ ] Ship `hello-selection` example

**Exit criteria:** Example plugin runs from selection menu, logs asset count, and the run appears in `history.jsonl` and the Developer page.

### Milestone 3 — Metadata + filesystem

- [ ] `renameAsset` via existing indexer/path update helpers
- [ ] `setTags` / `setRating` reusing command-layer logic
- [ ] Permission enforcement unit tests
- [ ] Ship `rename-by-date` example

**Exit criteria:** Rename example works on a test library; DB and disk stay consistent.

### Milestone 4 — Export hook

- [ ] `exportAssets` wrapping `export::export_assets_to_zip`
- [ ] Ship `export-web-ready` example
- [ ] Selection toolbar `Plugins ▾` submenu polish

**Exit criteria:** End-to-end demo: select → plugin export → ZIP on disk.

### Deferred (v1.5+)

- Import hooks (`onAssetIndexed` — opt-in, user-approved)
- `contributions.filters` (custom search facets)
- WASM modules for CPU-heavy transforms (watermark, hash)
- Signed plugin bundles
- Export to shared plugin folder from Settings

---

## Testing strategy

| Layer | What to test |
| --- | --- |
| Rust unit | Manifest parse, permission matrix, path validation |
| Rust unit | `history.rs`: write record, read records, trim to 200, clear |
| Rust integration | `run_plugin_action` with temp plugin dir + temp DB — verify history appended |
| Rust integration | Cancelled + timeout runs — verify `outcome` field in history record |
| JS contract | Example plugins in CI (headless `run_plugin_action`) |
| Manual | Settings install, selection menu, cancel mid-run, delete plugin folder |
| Manual | Settings → Extensions: last run badge, expandable history, "Clear history" |

**Commands:**

```bash
cd src-tauri && cargo test plugins::
bun run typecheck
```

---

## SPEC.md checklist (to add when work starts)

```markdown
### Plugins (Phase 3)

- [ ] Plugin registry (`{app_data}/plugins/`, manifest validation)
- [ ] Sandboxed JS host (`apiVersion: 1`, permission tokens)
- [ ] Selection actions in toolbar
- [ ] Settings → Extensions (install / enable / remove / history per plugin)
- [ ] Plugin execution history (`history.jsonl`, `getHistory()`, `clearHistory()`)
- [ ] First-party examples: hello, rename-by-date, export-web-ready, organize-by-template
- [ ] Developer diagnostics for plugin errors + history tail
```

---

## Open questions (resolve before Milestone 2)

All decisions below are resolved for v1 based on the table you provided:

1. **Install:** copy for users; developer paths/symlinks only in Developer Mode.
2. **Undo:** yes — plugin writes integrate with the same history system as native edits.
3. **Vault:** read-only in v1 (plugins cannot modify or destructively act on vault-encrypted assets).
4. **Direct DB Access:** never — no SQLite handles, query primitives, or schema access in JS.
5. **Asset Objects:** immutable snapshots only (value objects from `getAssets()`).
6. **Runtime:** fresh runtime per action.
7. **Transactions:** per-asset transactions with progress reporting.
8. **Progress API:** reserve a `reportProgress()` hook now for long-running actions.
9. **Preview Mode:** destructive operations require preview mode (host runs preview then apply).
10. **Execution history:** the host records every plugin run — outcome, affected asset count, duration, and logs — in a per-plugin execution history file. Visible on the Developer page and queryable via `lumora.getHistory()`. Never silently discarded.

---

## Related docs

- [`SPEC.md`](../SPEC.md) — Phase 3 roadmap
- [`adr/0002-plugin-host.md`](./adr/0002-plugin-host.md) — architecture decision
- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — dev setup
