export const STARTER_MAIN_JS = `/**
 * My plugin
 *
 * Edit runAction() below. Permissions are inferred when you save.
 */

export async function runAction(actionId, context) {
  lumora.log("info", \`\${actionId}: \${context.assetIds.length} asset(s) selected\`);
  context.reportProgress(0, context.assetIds.length);

  for (let i = 0; i < context.assetIds.length; i++) {
    lumora.log("info", \`  [\${i + 1}] \${context.assetIds[i]}\`);
    context.reportProgress(i + 1, context.assetIds.length);
  }

  return {
    ok: true,
    message: \`Processed \${context.assetIds.length} asset(s)\`,
  };
}
`;

export const PERMISSION_LABELS: Record<string, string> = {
  "read:assets": "Read Assets",
  "read:metadata": "Read Metadata",
  "write:metadata": "Write Metadata",
  "rename:filesystem": "Rename Files",
  "move:filesystem": "Move Files",
  "copy:filesystem": "Copy Files",
  "delete:filesystem": "Delete Files",
  "export:assets": "Export Assets",
};

export const PERMISSION_HINTS: Record<string, string> = {
  "read:assets": "Load selected photo paths and ids",
  "read:metadata": "EXIF, ratings, camera info",
  "write:metadata": "Update ratings and tags",
  "rename:filesystem": "Rename files on disk",
  "move:filesystem": "Move files into folders",
  "copy:filesystem": "Copy files (future API)",
  "delete:filesystem": "Delete files (future API)",
  "export:assets": "Export resized copies",
};
