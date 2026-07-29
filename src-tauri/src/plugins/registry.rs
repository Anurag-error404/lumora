//! Plugin registry: scan the plugins directory, resolve enabled state from preferences.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::plugins::manifest::PluginManifest;

/// A fully-resolved plugin entry returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    pub manifest: PluginManifest,
    /// Absolute path to the plugin directory.
    pub dir: String,
    /// Whether the plugin is currently enabled.
    pub enabled: bool,
    /// Whether an icon file exists.
    pub has_icon: bool,
    /// Whether a README file exists.
    pub has_readme: bool,
}

/// Scan `plugins_dir` and return valid plugin entries.
///
/// Invalid manifests are logged but not surfaced as errors so the UI
/// can still list healthy plugins.
pub fn scan(plugins_dir: &Path, enabled_map: &std::collections::HashMap<String, bool>) -> Vec<PluginEntry> {
    let Ok(read_dir) = std::fs::read_dir(plugins_dir) else {
        return Vec::new();
    };

    let mut entries: Vec<PluginEntry> = read_dir
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|dir_entry| {
            let dir = dir_entry.path();
            match PluginManifest::load(&dir) {
                Ok(manifest) => {
                    let enabled = enabled_map.get(&manifest.id).copied().unwrap_or(false);
                    let has_icon = dir.join("icon.png").exists();
                    let has_readme = dir.join("README.md").exists();
                    Some(PluginEntry {
                        manifest,
                        dir: dir.display().to_string(),
                        enabled,
                        has_icon,
                        has_readme,
                    })
                }
                Err(e) => {
                    tracing::warn!(dir = %dir.display(), error = %e, "skipping invalid plugin");
                    None
                }
            }
        })
        .collect();

    // Sort alphabetically by plugin id for a stable UI order.
    entries.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    entries
}

/// Install a plugin by copying its folder into `plugins_dir`.
///
/// If the chosen folder has no manifest but contains sub-folders that do,
/// all valid sub-folders are installed as a batch (allows selecting a parent
/// like `plugins/examples/` instead of each plugin individually).
///
/// Returns the list of installed plugin ids.
pub fn install_plugin_dir(source_dir: &Path, plugins_dir: &Path) -> AppResult<Vec<String>> {
    // Fast path: source_dir is itself a plugin folder.
    if source_dir.join(crate::plugins::manifest::MANIFEST_FILE).exists() {
        let id = install_single(source_dir, plugins_dir)?;
        return Ok(vec![id]);
    }

    // Slow path: scan immediate children for plugin folders (batch install).
    let mut installed = Vec::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(source_dir) else {
        return Err(crate::error::AppError::msg(format!(
            "PLUGIN_INVALID_MANIFEST: no {} found in {} and folder is unreadable",
            crate::plugins::manifest::MANIFEST_FILE,
            source_dir.display()
        )));
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let sub = entry.path();
        if !sub.join(crate::plugins::manifest::MANIFEST_FILE).exists() {
            continue;
        }
        match install_single(&sub, plugins_dir) {
            Ok(id) => installed.push(id),
            Err(e) => errors.push(format!("{}: {e}", sub.display())),
        }
    }

    if installed.is_empty() {
        let hint = if errors.is_empty() {
            format!(
                "PLUGIN_INVALID_MANIFEST: no {} found in {} or any immediate sub-folder",
                crate::plugins::manifest::MANIFEST_FILE,
                source_dir.display()
            )
        } else {
            format!(
                "PLUGIN_INVALID_MANIFEST: found sub-folders but all failed validation:\n{}",
                errors.join("\n")
            )
        };
        return Err(crate::error::AppError::msg(hint));
    }

    if !errors.is_empty() {
        tracing::warn!("Some sub-folders failed validation: {:?}", errors);
    }
    Ok(installed)
}

fn install_single(source_dir: &Path, plugins_dir: &Path) -> AppResult<String> {
    let manifest = PluginManifest::load(source_dir)?;
    let dest = plugins_dir.join(&manifest.id);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    copy_dir_all(source_dir, &dest)?;
    tracing::info!(plugin = %manifest.id, "plugin installed");
    Ok(manifest.id)
}

/// Remove a plugin folder and any registry state.
pub fn remove_plugin_dir(plugin_id: &str, plugins_dir: &Path) -> AppResult<()> {
    let dir = plugins_dir.join(plugin_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

/// Recursively copy a directory tree.
pub fn copy_dir_all(src: &Path, dst: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

/// Resolve the directory for a specific installed plugin.
pub fn plugin_dir(plugin_id: &str, plugins_dir: &Path) -> PathBuf {
    plugins_dir.join(plugin_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_minimal_manifest(dir: &Path, id: &str) {
        let content = format!(
            r#"{{
              "id": "{id}",
              "name": "Test",
              "version": "1.0.0",
              "apiVersion": 1,
              "permissions": [],
              "contributions": {{"actions": []}}
            }}"#
        );
        fs::write(dir.join("lumora.plugin.json"), content).unwrap();
    }

    #[test]
    fn scan_finds_plugins() {
        let tmp = tempdir().unwrap();
        let p1 = tmp.path().join("com.test.one");
        let p2 = tmp.path().join("com.test.two");
        fs::create_dir_all(&p1).unwrap();
        fs::create_dir_all(&p2).unwrap();
        write_minimal_manifest(&p1, "com.test.one");
        write_minimal_manifest(&p2, "com.test.two");

        let mut enabled = std::collections::HashMap::new();
        enabled.insert("com.test.one".into(), true);

        let entries = scan(tmp.path(), &enabled);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.manifest.id == "com.test.one" && e.enabled));
        assert!(entries.iter().any(|e| e.manifest.id == "com.test.two" && !e.enabled));
    }

    #[test]
    fn scan_skips_invalid() {
        let tmp = tempdir().unwrap();
        let bad = tmp.path().join("not-a-plugin");
        fs::create_dir_all(&bad).unwrap();
        // No manifest file — should be skipped silently.
        let entries = scan(tmp.path(), &Default::default());
        assert!(entries.is_empty());
    }

    #[test]
    fn install_copies_single_folder() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("com.test.myplugin");
        let plugins_dir = tmp.path().join("plugins");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&plugins_dir).unwrap();
        write_minimal_manifest(&src, "com.test.myplugin");

        let ids = install_plugin_dir(&src, &plugins_dir).unwrap();
        assert_eq!(ids, vec!["com.test.myplugin"]);
        assert!(plugins_dir.join("com.test.myplugin").join("lumora.plugin.json").exists());
    }

    #[test]
    fn install_batch_from_parent_dir() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("my-plugins");
        let plugins_dir = tmp.path().join("plugins");
        fs::create_dir_all(&plugins_dir).unwrap();

        // Two valid plugin sub-folders.
        let p1 = parent.join("com.test.alpha");
        let p2 = parent.join("com.test.beta");
        fs::create_dir_all(&p1).unwrap();
        fs::create_dir_all(&p2).unwrap();
        write_minimal_manifest(&p1, "com.test.alpha");
        write_minimal_manifest(&p2, "com.test.beta");

        let mut ids = install_plugin_dir(&parent, &plugins_dir).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["com.test.alpha", "com.test.beta"]);
    }
}
