export type PluginValidationIssue = {
  severity: "error" | "warn";
  code: string;
  message: string;
  line?: number;
};

export type PluginAnalysis = {
  permissions: string[];
  issues: PluginValidationIssue[];
  warnings: PluginValidationIssue[];
  hasRunAction: boolean;
  hasExport: boolean;
};

function issue(
  severity: "error" | "warn",
  code: string,
  message: string,
  line?: number,
): PluginValidationIssue {
  return { severity, code, message, line };
}

function findLine(source: string, needle: string): number | undefined {
  const idx = source.split("\n").findIndex((line) => line.includes(needle));
  return idx >= 0 ? idx + 1 : undefined;
}

export function analyzePluginSource(mainJs: string): PluginAnalysis {
  const permissions = new Set<string>();
  const issues: PluginValidationIssue[] = [];
  const warnings: PluginValidationIssue[] = [];

  const hasRunAction = mainJs.includes("runAction");
  const hasExport = mainJs.includes("export") && hasRunAction;

  if (!hasRunAction) {
    issues.push(
      issue(
        "error",
        "MISSING_RUN_ACTION",
        "main.js must define a runAction(actionId, context) function",
      ),
    );
  }

  if (hasRunAction && !hasExport) {
    warnings.push(
      issue(
        "warn",
        "MISSING_EXPORT",
        "Consider exporting runAction: export async function runAction(...)",
        findLine(mainJs, "runAction"),
      ),
    );
  }

  if (mainJs.includes("fetch(") || mainJs.includes("XMLHttpRequest")) {
    issues.push(
      issue(
        "error",
        "NETWORK_USAGE",
        "Network access is not allowed in plugins (remove fetch / XMLHttpRequest)",
        findLine(mainJs, "fetch"),
      ),
    );
  }

  if (mainJs.includes("require(") || mainJs.includes("import ")) {
    warnings.push(
      issue(
        "warn",
        "MODULE_IMPORT",
        "Plugins run as a single script — external imports are not supported",
      ),
    );
  }

  if (mainJs.includes("getAssets")) permissions.add("read:assets");
  if (mainJs.includes("renameAsset")) permissions.add("rename:filesystem");
  if (mainJs.includes("setRating") || mainJs.includes("setTags")) {
    permissions.add("write:metadata");
  }
  if (mainJs.includes("moveAssets") || mainJs.includes("organizeAssets")) {
    permissions.add("move:filesystem");
  }
  if (mainJs.includes("copyAssets")) permissions.add("copy:filesystem");
  if (mainJs.includes("exportAssets")) permissions.add("export:assets");

  const metadataSignals = [
    ".capturedAt",
    ".createdAt",
    ".rating",
    ".camera",
    ".lens",
    ".colorLabel",
    ".vaultLocked",
  ];
  if (metadataSignals.some((s) => mainJs.includes(s))) {
    permissions.add("read:metadata");
  }

  return {
    permissions: [...permissions].sort(),
    issues,
    warnings,
    hasRunAction,
    hasExport,
  };
}

export function validateManifestFields(
  id: string,
  name: string,
  actionId: string,
  actionLabel: string,
): PluginValidationIssue[] {
  const issues: PluginValidationIssue[] = [];
  if (!id.trim()) issues.push(issue("error", "MISSING_ID", "Plugin id is required"));
  if (!name.trim()) issues.push(issue("error", "MISSING_NAME", "Plugin name is required"));
  if (!actionId.trim()) {
    issues.push(issue("error", "MISSING_ACTION_ID", "Action id is required"));
  }
  if (!actionLabel.trim()) {
    issues.push(issue("error", "MISSING_ACTION_LABEL", "Action label is required"));
  }
  return issues;
}

export function analyzeDraft(
  mainJs: string,
  id: string,
  name: string,
  actionId: string,
  actionLabel: string,
): PluginAnalysis {
  const base = analyzePluginSource(mainJs);
  for (const issue of validateManifestFields(id, name, actionId, actionLabel)) {
    if (issue.severity === "error") base.issues.push(issue);
    else base.warnings.push(issue);
  }
  return base;
}

export function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
}

export function suggestForkId(sourceId: string): string {
  const slug = slugify(sourceId.split(".").pop() ?? "fork");
  return `com.personal.fork-${slug}`;
}
