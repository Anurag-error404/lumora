//! Read, save, and fork plugin source files in the user's plugins directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::plugins::analyze::{analyze_main_js, validate_manifest_fields, PluginAnalysis};
use crate::plugins::manifest::{ActionContribution, Contributions, PluginManifest, MANIFEST_FILE};
use crate::plugins::registry::{copy_dir_all, plugin_dir};
use crate::plugins::scaffold::{is_valid_plugin_id, render_readme};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSources {
    pub plugin_id: String,
    pub dir: String,
    pub main_js: String,
    pub manifest: PluginManifest,
    pub readme: Option<String>,
    pub forked_from: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePluginDraft {
    pub plugin_id: String,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub action_id: String,
    pub action_label: String,
    pub main_js: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePluginResult {
    pub id: String,
    pub dir: String,
    pub analysis: PluginAnalysis,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkPluginSpec {
    /// Installed plugin id to copy from.
    pub source_plugin_id: Option<String>,
    /// Absolute path to example / external plugin folder.
    pub source_dir: Option<String>,
    pub new_id: String,
    pub new_name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub action_id: Option<String>,
    pub action_label: Option<String>,
    pub main_js: Option<String>,
}

pub fn read_sources(plugins_dir: &Path, plugin_id: &str) -> AppResult<PluginSources> {
    let dir = plugin_dir(plugin_id, plugins_dir);
    if !dir.exists() {
        return Err(AppError::msg(format!(
            "PLUGIN_NOT_FOUND: plugin '{plugin_id}' is not installed"
        )));
    }
    read_sources_from_dir(&dir, Some(plugin_id.to_string()))
}

pub fn read_sources_from_dir(dir: &Path, plugin_id: Option<String>) -> AppResult<PluginSources> {
    let manifest = PluginManifest::load(dir)?;
    let main_path = dir.join(manifest.main.as_deref().unwrap_or("main.js"));
    let main_js = std::fs::read_to_string(&main_path).map_err(|e| {
        AppError::msg(format!(
            "PLUGIN_RUNTIME_ERROR: cannot read {}: {e}",
            main_path.display()
        ))
    })?;
    let readme_path = dir.join("README.md");
    let readme = if readme_path.exists() {
        Some(std::fs::read_to_string(&readme_path)?)
    } else {
        None
    };
    let forked_from = readme
        .as_deref()
        .and_then(|text| {
            text.lines()
                .find(|l| l.starts_with("Forked from:"))
                .map(|l| l.trim_start_matches("Forked from:").trim().to_string())
        });

    Ok(PluginSources {
        plugin_id: plugin_id.unwrap_or_else(|| manifest.id.clone()),
        dir: dir.display().to_string(),
        main_js,
        manifest,
        readme,
        forked_from,
    })
}

pub fn save_draft(plugins_dir: &Path, draft: SavePluginDraft) -> AppResult<SavePluginResult> {
    let plugin_id = draft.plugin_id.trim().to_string();
    let dir = plugin_dir(&plugin_id, plugins_dir);
    if !dir.exists() {
        return Err(AppError::msg(format!(
            "PLUGIN_NOT_FOUND: plugin '{plugin_id}' is not installed"
        )));
    }

    let analysis = validate_and_build_analysis(
        &plugin_id,
        &draft.name,
        &draft.action_id,
        &draft.action_label,
        &draft.main_js,
    )?;
    if analysis.issues.iter().any(|i| i.severity == "error") {
        let msg = analysis
            .issues
            .iter()
            .map(|i| i.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AppError::msg(format!("PLUGIN_INVALID_MANIFEST: {msg}")));
    }

    write_plugin_files(
        &dir,
        &plugin_id,
        &draft.name,
        draft.description.as_deref(),
        draft.author.as_deref(),
        &draft.action_id,
        &draft.action_label,
        &draft.main_js,
        &analysis.permissions,
        None,
    )?;

    Ok(SavePluginResult {
        id: plugin_id.clone(),
        dir: dir.display().to_string(),
        analysis,
    })
}

pub fn fork_plugin(plugins_dir: &Path, spec: ForkPluginSpec) -> AppResult<SavePluginResult> {
    let new_id = spec.new_id.trim().to_string();
    let new_name = spec.new_name.trim().to_string();

    if !is_valid_plugin_id(&new_id) {
        return Err(AppError::msg(
            "Plugin id must be reverse-DNS, e.g. com.personal.my-fork",
        ));
    }

    let dest = plugins_dir.join(&new_id);
    if dest.exists() {
        return Err(AppError::msg(format!(
            "A plugin with id '{new_id}' already exists"
        )));
    }

    let (source_label, source_path) = resolve_fork_source(plugins_dir, &spec)?;
    std::fs::create_dir_all(&dest)?;
    copy_dir_all(&source_path, &dest)?;

    let source_manifest = PluginManifest::load(&dest)?;
    let action_id = spec
        .action_id
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| source_manifest.contributions.actions.first().map(|a| a.id.clone()).unwrap_or_else(|| "action".into()));
    let action_label = spec
        .action_label
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            source_manifest
                .contributions
                .actions
                .first()
                .map(|a| a.label.clone())
                .unwrap_or_else(|| format!("{new_name}…"))
        });

    let main_js = if let Some(js) = spec.main_js.filter(|s| !s.trim().is_empty()) {
        js
    } else {
        let main_file = dest.join(source_manifest.main.as_deref().unwrap_or("main.js"));
        std::fs::read_to_string(&main_file)?
    };

    let description = spec.description.clone().or_else(|| {
        Some(format!(
            "Personal fork of {} ({})",
            source_manifest.name, source_label
        ))
    });

    let analysis = validate_and_build_analysis(
        &new_id,
        &new_name,
        &action_id,
        &action_label,
        &main_js,
    )?;
    if analysis.issues.iter().any(|i| i.severity == "error") {
        let _ = std::fs::remove_dir_all(&dest);
        let msg = analysis
            .issues
            .iter()
            .map(|i| i.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AppError::msg(format!("PLUGIN_INVALID_MANIFEST: {msg}")));
    }

    write_plugin_files(
        &dest,
        &new_id,
        &new_name,
        description.as_deref(),
        spec.author.as_deref(),
        &action_id,
        &action_label,
        &main_js,
        &analysis.permissions,
        Some(&source_label),
    )?;

    Ok(SavePluginResult {
        id: new_id,
        dir: dest.display().to_string(),
        analysis,
    })
}

pub fn write_plugin_files(
    dir: &Path,
    id: &str,
    name: &str,
    description: Option<&str>,
    author: Option<&str>,
    action_id: &str,
    action_label: &str,
    main_js: &str,
    permissions: &[String],
    forked_from: Option<&str>,
) -> AppResult<PluginManifest> {
    let description = description
        .unwrap_or("A custom Lumora selection action.")
        .trim()
        .to_string();
    let author = author.unwrap_or("You").trim().to_string();

    let manifest = PluginManifest {
        schema: None,
        id: id.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        api_version: 1,
        description: description.clone(),
        author: author.clone(),
        permissions: permissions.to_vec(),
        contributions: Contributions {
            actions: vec![ActionContribution {
                id: action_id.to_string(),
                label: action_label.to_string(),
                scope: "selection".to_string(),
                min_selection: 1,
                max_selection: 500,
            }],
        },
        main: Some("main.js".to_string()),
    };

    manifest.validate(dir)?;

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| AppError::msg(format!("manifest encode failed: {e}")))?;
    std::fs::write(dir.join(MANIFEST_FILE), manifest_json)?;
    std::fs::write(dir.join("main.js"), main_js)?;

    let mut readme = render_readme(&manifest, action_id, action_label);
    if let Some(from) = forked_from {
        readme = format!("Forked from: {from}\n\n{readme}");
    }
    std::fs::write(dir.join("README.md"), readme)?;

    Ok(manifest)
}

fn validate_and_build_analysis(
    id: &str,
    name: &str,
    action_id: &str,
    action_label: &str,
    main_js: &str,
) -> AppResult<PluginAnalysis> {
    let mut analysis = analyze_main_js(main_js);
    for issue in validate_manifest_fields(id, name, action_id, action_label) {
        if issue.severity == "error" {
            analysis.issues.push(issue);
        } else {
            analysis.warnings.push(issue);
        }
    }
    Ok(analysis)
}

fn resolve_fork_source(plugins_dir: &Path, spec: &ForkPluginSpec) -> AppResult<(String, PathBuf)> {
    if let Some(id) = spec.source_plugin_id.as_ref().filter(|s| !s.trim().is_empty()) {
        let path = plugin_dir(id, plugins_dir);
        if !path.exists() {
            return Err(AppError::msg(format!("PLUGIN_NOT_FOUND: {id}")));
        }
        return Ok((id.clone(), path));
    }
    if let Some(dir) = spec.source_dir.as_ref().filter(|s| !s.trim().is_empty()) {
        let path = PathBuf::from(dir);
        let manifest = PluginManifest::load(&path)?;
        return Ok((manifest.id.clone(), path));
    }
    Err(AppError::msg("Fork source plugin id or directory is required"))
}
