/**
 * Hello Selection — minimal lifecycle example.
 *
 * No permissions, no destructive writes, no preview required.
 * Demonstrates: runAction signature, reportProgress, lumora.log.
 */
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
