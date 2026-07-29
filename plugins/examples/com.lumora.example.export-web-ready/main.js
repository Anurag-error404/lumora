/**
 * Export — Web Ready
 *
 * Exports selected photos as web-optimised JPEG (max 2048 px, quality 85)
 * packaged in a ZIP archive.  Demonstrates:
 *   - export:assets permission
 *   - preview/apply pattern with no-file-I/O preview
 *   - vault-locked asset skipping
 *   - progress framing around a host-managed export
 */

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

  // ── Preview phase ──────────────────────────────────────────────────────────
  // No file I/O in preview — just report what will be exported.
  if (context.mode === "preview") {
    const exportable = assets.filter((a) => !a.vaultLocked);
    const skipped = total - exportable.length;
    return {
      ok: true,
      message: `Will export ${exportable.length} photo(s) as web-ready JPEG${
        skipped > 0 ? ` (${skipped} vault-locked asset(s) skipped)` : ""
      }.`,
    };
  }

  // ── Apply phase ────────────────────────────────────────────────────────────
  const exportIds = [];
  for (const asset of assets) {
    if (asset.vaultLocked) {
      lumora.log("warn", `skip vault-locked asset: ${asset.id}`);
      continue;
    }
    exportIds.push(asset.id);
  }

  if (exportIds.length === 0) {
    return {
      ok: false,
      message: "All selected assets are vault-locked; nothing to export.",
    };
  }

  context.reportProgress(0, exportIds.length);

  // exportAssets opens a save-panel for the user then writes a ZIP.
  // Progress is reported inside the host as it processes each file;
  // we frame the operation with start + done here.
  const result = await lumora.exportAssets(exportIds, EXPORT_OPTIONS);

  context.reportProgress(exportIds.length, exportIds.length);
  lumora.log(
    "info",
    `export-web-ready: ${result.exportedCount} file(s) → ${result.destinationPath}`
  );

  return {
    ok: true,
    message: `Exported ${result.exportedCount} photo(s).`,
  };
}
