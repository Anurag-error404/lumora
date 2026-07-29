/**
 * Rename by Capture Date
 *
 * Renames selected files to `YYYY-MM-DD_<shortid>.<ext>` using EXIF capture
 * date.  Demonstrates:
 *   - read:metadata + rename:filesystem permissions
 *   - preview/apply two-phase pattern
 *   - vault-locked asset handling
 *   - per-asset progress reporting
 *   - undo-friendly rename (host records each rename in history)
 */

/** Build the new filename for an asset. Returns null when there is no date. */
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

  // ── Preview phase ──────────────────────────────────────────────────────────
  // Validate + stage only; no disk or DB changes.
  if (context.mode === "preview") {
    const plan = [];
    for (const asset of assets) {
      if (asset.vaultLocked) {
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

  // ── Apply phase ────────────────────────────────────────────────────────────
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
    // renameAsset is per-asset transactional — a failure here does not affect
    // assets already renamed.  The host records each rename in the undo history
    // so the user can revert the whole batch.
    await lumora.renameAsset(asset.id, newName);
    renamed++;
  }

  context.reportProgress(total, total);
  return {
    ok: true,
    message: `Renamed ${renamed} file(s), skipped ${skipped}.`,
  };
}
