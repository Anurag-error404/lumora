//! Static analysis for plugin JavaScript — permission inference and structure checks.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginValidationIssue {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAnalysis {
    pub permissions: Vec<String>,
    pub issues: Vec<PluginValidationIssue>,
    pub warnings: Vec<PluginValidationIssue>,
    pub has_run_action: bool,
    pub has_export: bool,
}

const KNOWN_PERMISSIONS: &[&str] = &[
    "read:assets",
    "read:metadata",
    "write:metadata",
    "rename:filesystem",
    "move:filesystem",
    "copy:filesystem",
    "delete:filesystem",
    "export:assets",
];

pub fn analyze_main_js(source: &str) -> PluginAnalysis {
    let mut permissions = std::collections::BTreeSet::new();
    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    let has_run_action = source.contains("runAction");
    let has_export = source.contains("export") && source.contains("runAction");

    if !has_run_action {
        issues.push(issue(
            "error",
            "MISSING_RUN_ACTION",
            "main.js must define a runAction(actionId, context) function",
            None,
        ));
    }

    if has_run_action && !has_export {
        warnings.push(issue(
            "warn",
            "MISSING_EXPORT",
            "Consider exporting runAction: export async function runAction(...)",
            find_line(source, "runAction"),
        ));
    }

    if source.contains("fetch(") || source.contains("XMLHttpRequest") {
        issues.push(issue(
            "error",
            "NETWORK_USAGE",
            "Network access is not allowed in plugins (remove fetch / XMLHttpRequest)",
            find_line(source, "fetch"),
        ));
    }

    if source.contains("require(") || source.contains("import ") {
        warnings.push(issue(
            "warn",
            "MODULE_IMPORT",
            "Plugins run as a single script — external imports are not supported",
            None,
        ));
    }

    // API → permission mapping
    if source.contains("getAssets") {
        permissions.insert("read:assets".to_string());
    }
    if source.contains("renameAsset") {
        permissions.insert("rename:filesystem".to_string());
    }
    if source.contains("setRating") || source.contains("setTags") {
        permissions.insert("write:metadata".to_string());
    }
    if source.contains("moveAssets") || source.contains("organizeAssets") {
        permissions.insert("move:filesystem".to_string());
    }
    if source.contains("copyAssets") {
        permissions.insert("copy:filesystem".to_string());
    }
    if source.contains("exportAssets") {
        permissions.insert("export:assets".to_string());
    }

    // Metadata field reads on asset objects
    let metadata_signals = [
        ".capturedAt",
        ".createdAt",
        ".rating",
        ".camera",
        ".lens",
        ".colorLabel",
        ".vaultLocked",
    ];
    if metadata_signals.iter().any(|s| source.contains(s)) {
        permissions.insert("read:metadata".to_string());
    }

    PluginAnalysis {
        permissions: permissions.into_iter().collect(),
        issues,
        warnings,
        has_run_action,
        has_export,
    }
}

pub fn validate_manifest_fields(
    id: &str,
    name: &str,
    action_id: &str,
    action_label: &str,
) -> Vec<PluginValidationIssue> {
    let mut issues = Vec::new();
    if id.trim().is_empty() {
        issues.push(issue("error", "MISSING_ID", "Plugin id is required", None));
    }
    if name.trim().is_empty() {
        issues.push(issue("error", "MISSING_NAME", "Plugin name is required", None));
    }
    if action_id.trim().is_empty() {
        issues.push(issue(
            "error",
            "MISSING_ACTION_ID",
            "Action id is required",
            None,
        ));
    }
    if action_label.trim().is_empty() {
        issues.push(issue(
            "error",
            "MISSING_ACTION_LABEL",
            "Action label is required",
            None,
        ));
    }
    issues
}

pub fn known_permissions() -> &'static [&'static str] {
    KNOWN_PERMISSIONS
}

fn issue(severity: &str, code: &str, message: &str, line: Option<u32>) -> PluginValidationIssue {
    PluginValidationIssue {
        severity: severity.to_string(),
        code: code.to_string(),
        message: message.to_string(),
        line,
    }
}

fn find_line(source: &str, needle: &str) -> Option<u32> {
    source
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(needle))
        .map(|(i, _)| (i + 1) as u32)
}
