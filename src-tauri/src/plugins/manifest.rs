//! Parse and validate `lumora.plugin.json` manifest files.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub const MANIFEST_FILE: &str = "lumora.plugin.json";
pub const SUPPORTED_API_VERSION: u32 = 1;
pub const MAX_SELECTION: u32 = 500;

/// Full parsed manifest from `lumora.plugin.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub permissions: Vec<String>,
    pub contributions: Contributions,
    #[serde(default)]
    pub main: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Contributions {
    #[serde(default)]
    pub actions: Vec<ActionContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionContribution {
    pub id: String,
    pub label: String,
    /// `"selection"` is the only supported scope in v1.
    pub scope: String,
    #[serde(default = "default_min_selection")]
    pub min_selection: u32,
    #[serde(default = "default_max_selection")]
    pub max_selection: u32,
}

fn default_min_selection() -> u32 {
    1
}
fn default_max_selection() -> u32 {
    MAX_SELECTION
}

impl PluginManifest {
    /// Load and validate the manifest from a plugin directory.
    pub fn load(plugin_dir: &Path) -> AppResult<Self> {
        let path = plugin_dir.join(MANIFEST_FILE);
        if !path.exists() {
            return Err(AppError::msg(format!(
                "PLUGIN_INVALID_MANIFEST: missing {} in {}",
                MANIFEST_FILE,
                plugin_dir.display()
            )));
        }
        let raw = std::fs::read_to_string(&path)?;
        let manifest: PluginManifest = serde_json::from_str(&raw).map_err(|e| {
            AppError::msg(format!("PLUGIN_INVALID_MANIFEST: parse error: {e}"))
        })?;
        manifest.validate(plugin_dir)?;
        Ok(manifest)
    }

    pub fn validate(&self, plugin_dir: &Path) -> AppResult<()> {
        // id must be non-empty and folder name must match
        if self.id.is_empty() {
            return Err(AppError::msg("PLUGIN_INVALID_MANIFEST: id is empty"));
        }
        let folder_name = plugin_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if folder_name != self.id {
            return Err(AppError::msg(format!(
                "PLUGIN_INVALID_MANIFEST: folder name '{}' does not match manifest id '{}'",
                folder_name, self.id
            )));
        }

        // apiVersion must be supported
        if self.api_version != SUPPORTED_API_VERSION {
            return Err(AppError::msg(format!(
                "PLUGIN_API_MISMATCH: plugin requires apiVersion {} but host supports {}",
                self.api_version, SUPPORTED_API_VERSION
            )));
        }

        // All declared permissions must be known
        let known: HashSet<&str> = [
            "read:assets",
            "read:metadata",
            "write:metadata",
            "rename:filesystem",
            "move:filesystem",
            "copy:filesystem",
            "delete:filesystem",
            "export:assets",
        ]
        .into();
        for perm in &self.permissions {
            if !known.contains(perm.as_str()) {
                return Err(AppError::msg(format!(
                    "PLUGIN_INVALID_MANIFEST: unknown permission '{perm}'"
                )));
            }
        }

        // Validate action contributions
        for action in &self.contributions.actions {
            if action.id.is_empty() {
                return Err(AppError::msg(
                    "PLUGIN_INVALID_MANIFEST: action id is empty",
                ));
            }
            if action.scope != "selection" {
                return Err(AppError::msg(format!(
                    "PLUGIN_INVALID_MANIFEST: unsupported action scope '{}' (only 'selection' in v1)",
                    action.scope
                )));
            }
            if action.max_selection > MAX_SELECTION {
                return Err(AppError::msg(format!(
                    "PLUGIN_INVALID_MANIFEST: maxSelection {} exceeds hard limit {}",
                    action.max_selection, MAX_SELECTION
                )));
            }
            if action.min_selection == 0 {
                return Err(AppError::msg(
                    "PLUGIN_INVALID_MANIFEST: minSelection must be >= 1",
                ));
            }
        }

        // If actions are present, main script must be declared
        if !self.contributions.actions.is_empty() && self.main.is_none() {
            return Err(AppError::msg(
                "PLUGIN_INVALID_MANIFEST: 'main' is required when contributions.actions is non-empty",
            ));
        }

        Ok(())
    }

    /// Returns true if the plugin requires a JS entry point.
    pub fn has_script(&self) -> bool {
        self.main.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_manifest(dir: &Path, content: &str) {
        fs::write(dir.join(MANIFEST_FILE), content).unwrap();
    }

    #[test]
    fn valid_manifest_loads() {
        let tmp = tempdir().unwrap();
        let plugin_dir = tmp.path().join("com.test.plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            r#"{
              "id": "com.test.plugin",
              "name": "Test",
              "version": "1.0.0",
              "apiVersion": 1,
              "permissions": [],
              "contributions": { "actions": [] }
            }"#,
        );
        let m = PluginManifest::load(&plugin_dir).unwrap();
        assert_eq!(m.id, "com.test.plugin");
    }

    #[test]
    fn rejects_unknown_permission() {
        let tmp = tempdir().unwrap();
        let plugin_dir = tmp.path().join("com.test.plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            r#"{
              "id": "com.test.plugin",
              "name": "Test",
              "version": "1.0.0",
              "apiVersion": 1,
              "permissions": ["network:unrestricted"],
              "contributions": {}
            }"#,
        );
        let err = PluginManifest::load(&plugin_dir).unwrap_err();
        assert!(err.to_string().contains("unknown permission"));
    }

    #[test]
    fn rejects_folder_name_mismatch() {
        let tmp = tempdir().unwrap();
        let plugin_dir = tmp.path().join("com.wrong.name");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            r#"{
              "id": "com.correct.name",
              "name": "Test",
              "version": "1.0.0",
              "apiVersion": 1,
              "permissions": [],
              "contributions": {}
            }"#,
        );
        let err = PluginManifest::load(&plugin_dir).unwrap_err();
        assert!(err.to_string().contains("folder name"));
    }

    #[test]
    fn rejects_wrong_api_version() {
        let tmp = tempdir().unwrap();
        let plugin_dir = tmp.path().join("com.test.plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        write_manifest(
            &plugin_dir,
            r#"{
              "id": "com.test.plugin",
              "name": "Test",
              "version": "1.0.0",
              "apiVersion": 99,
              "permissions": [],
              "contributions": {}
            }"#,
        );
        let err = PluginManifest::load(&plugin_dir).unwrap_err();
        assert!(err.to_string().contains("PLUGIN_API_MISMATCH"));
    }
}
