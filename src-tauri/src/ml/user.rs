//! Bring-your-own local ONNX backends (user-imported options).

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::evaluate::{self, EvalReport};
use super::library::Capability;
use super::profiles::CLIP_EMBED_DIM;
use crate::error::{AppError, AppResult};
use crate::tags::TagsModelPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelOption {
    pub id: String,
    pub capability: String,
    pub name: String,
    pub summary: String,
    pub input_size: Option<u32>,
    pub embedding_dim: Option<i64>,
    pub primary_path: String,
    pub labels_path: Option<String>,
    pub text_path: Option<String>,
    pub tokenizer_path: Option<String>,
    pub created_at: String,
}

pub fn is_user_option_id(id: &str) -> bool {
    id.starts_with("user-")
}

fn copy_into_user_dir(models_dir: &Path, option_id: &str, src: &Path, file_name: &str) -> AppResult<PathBuf> {
    let dir = models_dir.join("user").join(option_id);
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join(file_name);
    std::fs::copy(src, &dest).map_err(|e| {
        AppError::msg(format!(
            "copy {} → {}: {e}",
            src.display(),
            dest.display()
        ))
    })?;
    Ok(dest)
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserModelOption> {
    Ok(UserModelOption {
        id: r.get(0)?,
        capability: r.get(1)?,
        name: r.get(2)?,
        summary: r.get(3)?,
        input_size: r.get(4)?,
        embedding_dim: r.get(5)?,
        primary_path: r.get(6)?,
        labels_path: r.get(7)?,
        text_path: r.get(8)?,
        tokenizer_path: r.get(9)?,
        created_at: r.get(10)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<UserModelOption>> {
    let row = conn
        .query_row(
            "SELECT id, capability, name, summary, input_size, embedding_dim,
                    primary_path, labels_path, text_path, tokenizer_path, created_at
             FROM ml_user_options WHERE id = ?1",
            params![id],
            map_row,
        )
        .optional()?;
    Ok(row)
}

pub fn list(conn: &Connection) -> AppResult<Vec<UserModelOption>> {
    let mut stmt = conn.prepare(
        "SELECT id, capability, name, summary, input_size, embedding_dim,
                primary_path, labels_path, text_path, tokenizer_path, created_at
         FROM ml_user_options
         ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[allow(dead_code)]
pub fn list_for_capability(conn: &Connection, capability: Capability) -> AppResult<Vec<UserModelOption>> {
    let mut stmt = conn.prepare(
        "SELECT id, capability, name, summary, input_size, embedding_dim,
                primary_path, labels_path, text_path, tokenizer_path, created_at
         FROM ml_user_options
         WHERE capability = ?1
         ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![capability.as_str()], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn insert(conn: &Connection, opt: &UserModelOption) -> AppResult<()> {
    conn.execute(
        "INSERT INTO ml_user_options (
            id, capability, name, summary, input_size, embedding_dim,
            primary_path, labels_path, text_path, tokenizer_path, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            opt.id,
            opt.capability,
            opt.name,
            opt.summary,
            opt.input_size,
            opt.embedding_dim,
            opt.primary_path,
            opt.labels_path,
            opt.text_path,
            opt.tokenizer_path,
            opt.created_at,
        ],
    )?;
    Ok(())
}

/// Evaluate + import a local AutoTags classifier.
pub fn import_autotags(
    conn: &Connection,
    models_dir: &Path,
    model_path: &Path,
    labels_path: &Path,
    display_name: Option<String>,
) -> AppResult<(UserModelOption, EvalReport)> {
    let report = evaluate::evaluate_autotags(model_path, labels_path)?;
    if !report.compatible {
        return Err(AppError::msg(format!(
            "model not compatible: {}",
            report.reasons.join("; ")
        )));
    }
    let input_size = report.input_size.unwrap_or(224);
    let id = format!("user-autotags-{}", Uuid::new_v4());
    let model_dest = copy_into_user_dir(models_dir, &id, model_path, "model.onnx")?;
    let labels_dest = copy_into_user_dir(models_dir, &id, labels_path, "labels.txt")?;
    let name = display_name.unwrap_or_else(|| {
        model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Custom classifier")
            .to_string()
    });
    let opt = UserModelOption {
        id: id.clone(),
        capability: Capability::AutoTags.as_str().to_string(),
        name,
        summary: format!("Local ONNX classifier · {input_size}×{input_size} · ImageNet-1000"),
        input_size: Some(input_size),
        embedding_dim: None,
        primary_path: model_dest.display().to_string(),
        labels_path: Some(labels_dest.display().to_string()),
        text_path: None,
        tokenizer_path: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    insert(conn, &opt)?;
    Ok((opt, report))
}

/// Evaluate + import a local CLIP vision/text/tokenizer bundle.
pub fn import_clip(
    conn: &Connection,
    models_dir: &Path,
    vision_path: &Path,
    text_path: &Path,
    tokenizer_path: &Path,
    display_name: Option<String>,
) -> AppResult<(UserModelOption, EvalReport)> {
    let report = evaluate::evaluate_clip_bundle(vision_path, text_path, tokenizer_path)?;
    if !report.compatible {
        return Err(AppError::msg(format!(
            "model not compatible: {}",
            report.reasons.join("; ")
        )));
    }
    let id = format!("user-clip-{}", Uuid::new_v4());
    let vision_dest = copy_into_user_dir(models_dir, &id, vision_path, "vision.onnx")?;
    let text_dest = copy_into_user_dir(models_dir, &id, text_path, "text.onnx")?;
    let tok_dest = copy_into_user_dir(models_dir, &id, tokenizer_path, "tokenizer.json")?;
    let name = display_name.unwrap_or_else(|| {
        vision_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Custom CLIP")
            .to_string()
    });
    let opt = UserModelOption {
        id: id.clone(),
        capability: Capability::SemanticSearch.as_str().to_string(),
        name,
        summary: format!("Local CLIP ONNX · embed dim {CLIP_EMBED_DIM}"),
        input_size: Some(224),
        embedding_dim: Some(CLIP_EMBED_DIM as i64),
        primary_path: vision_dest.display().to_string(),
        labels_path: None,
        text_path: Some(text_dest.display().to_string()),
        tokenizer_path: Some(tok_dest.display().to_string()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    insert(conn, &opt)?;
    Ok((opt, report))
}

#[allow(dead_code)]
pub fn remove(conn: &Connection, models_dir: &Path, id: &str) -> AppResult<()> {
    if !is_user_option_id(id) {
        return Err(AppError::msg("not a user model option"));
    }
    let opt = get(conn, id)?.ok_or_else(|| AppError::msg("user model not found"))?;
    conn.execute("DELETE FROM ml_user_options WHERE id = ?1", params![id])?;
    let dir = models_dir.join("user").join(id);
    let _ = std::fs::remove_dir_all(&dir);
    // Also try removing individual paths if outside the dir.
    let _ = opt;
    Ok(())
}

pub fn tags_paths(opt: &UserModelOption) -> AppResult<TagsModelPaths> {
    let labels = opt
        .labels_path
        .as_ref()
        .ok_or_else(|| AppError::msg("user auto-tags model missing labels path"))?;
    Ok(TagsModelPaths {
        model: PathBuf::from(&opt.primary_path),
        labels: PathBuf::from(labels),
        input_size: opt.input_size.unwrap_or(224),
    })
}

pub fn clip_paths(opt: &UserModelOption) -> AppResult<crate::semantic::SemanticModelPaths> {
    let text = opt
        .text_path
        .as_ref()
        .ok_or_else(|| AppError::msg("user CLIP missing text model"))?;
    let tokenizer = opt
        .tokenizer_path
        .as_ref()
        .ok_or_else(|| AppError::msg("user CLIP missing tokenizer"))?;
    Ok(crate::semantic::SemanticModelPaths {
        image: PathBuf::from(&opt.primary_path),
        text: PathBuf::from(text),
        tokenizer: PathBuf::from(tokenizer),
    })
}
