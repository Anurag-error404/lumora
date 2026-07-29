/**
 * Organize by Album Template
 *
 * Moves selected photos into a template-driven folder structure.
 * Demonstrates:
 *   - move:filesystem permission
 *   - preview plan (no execution) + apply with Undo/history
 *   - organizeAssets host API
 *   - progress reporting
 *
 * Template variables resolved by the host:
 *   ${year}      — 4-digit capture year
 *   ${month}     — 2-digit capture month
 *   ${day}       — 2-digit capture day
 *   ${filename}  — original file name (with extension)
 *   ${album}     — primary album name for the asset, or "Unsorted"
 */

// Change this template to customise the folder structure.
const OPTIONS = {
  strategy: "template",
  template: "${year}/${album}/${filename}",
  mode: "move",
};

export async function runAction(actionId, context) {
  const total = context.assetIds.length;

  // ── Preview phase ──────────────────────────────────────────────────────────
  // Host computes the full plan (collisions, skips, final relative paths).
  // No disk or DB changes happen in preview mode.
  if (context.mode === "preview") {
    const preview = await lumora.organizeAssets({
      assetIds: context.assetIds,
      ...OPTIONS,
    });

    context.reportProgress(0, total);
    context.reportProgress(total, total);

    return {
      ok: true,
      message:
        preview?.message ?? `Will reorganize ${total} asset(s) (preview mode).`,
      previewPlan: preview?.plan,
    };
  }

  // ── Apply phase ────────────────────────────────────────────────────────────
  // Host executes the previously planned operation transactionally.
  // Supports Undo: the whole batch can be reversed via Edit → Undo.
  const result = await lumora.organizeAssets({
    assetIds: context.assetIds,
    ...OPTIONS,
  });

  return {
    ok: true,
    message:
      result?.message ??
      `Organized selection (moved ${result?.movedCount ?? "?"}, skipped ${
        result?.skippedCount ?? "?"
      }).`,
  };
}
