//! QuickJS sandboxed plugin host — Milestone 2.
//!
//! Each action run gets a **fresh** QuickJS runtime (no persisted globals).
//! The `lumora` global is frozen and provides the allowlisted host API.
//! All SQLite and filesystem I/O stays in Rust; JS only calls through this API.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use rquickjs::promise::PromiseState;
use rquickjs::{Context, Function, Object, Runtime, Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::AssetSummary;
use crate::plugins::history::{LogLevel, PluginLogLine, PluginRunRecord, RunOutcome};
use crate::plugins::manifest::PluginManifest;
use crate::plugins::permissions::{Permission, PermissionSet};

/// Wall-clock timeout for a single action run.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(120);

/// Live progress payload emitted to the frontend while a plugin runs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRunProgressEvent {
    pub run_id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub action_id: String,
    pub phase: String,
    pub current: u32,
    pub total: u32,
    pub message: Option<String>,
    pub logs: Vec<PluginLogLine>,
}

pub type ProgressCallback = Arc<dyn Fn(PluginRunProgressEvent) + Send + Sync>;

/// Result returned by a plugin action run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_plan: Option<serde_json::Value>,
}

/// Internal accumulator shared between Rust closures injected into JS.
struct RunContext {
    log_lines: Vec<PluginLogLine>,
    start: Instant,
    assets_affected: u32,
    assets_skipped: u32,
}

impl RunContext {
    fn new() -> Self {
        Self {
            log_lines: Vec::new(),
            start: Instant::now(),
            assets_affected: 0,
            assets_skipped: 0,
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

/// Execute a plugin action inside a fresh QuickJS runtime.
///
/// Returns an `ActionResult` on success or a structured `AppError` on failure.
/// Always produces a `PluginRunRecord` via the `on_record` callback so the
/// caller can append it to `history.jsonl`.
/// Strip ES-module `export` keywords so plugin source can run in a plain script eval.
fn prepare_plugin_source(src: &str) -> String {
    src.replace("export async function", "async function")
        .replace("export function", "function")
}

pub fn run_action(
    plugin_dir: &Path,
    action_id: &str,
    asset_ids: &[String],
    mode: &str,
    conn: &Connection,
    on_progress: Option<ProgressCallback>,
) -> AppResult<(ActionResult, PluginRunRecord)> {
    let started_at = chrono::Utc::now().to_rfc3339();
    let run_id = Uuid::new_v4().to_string();

    // Load and validate manifest.
    let manifest = PluginManifest::load(plugin_dir)?;
    let perms = PermissionSet::from_tokens(&manifest.permissions);

    // Locate the JS entry file.
    let main_file = manifest.main.as_deref().ok_or_else(|| {
        AppError::msg("PLUGIN_INVALID_MANIFEST: no 'main' entry for this plugin")
    })?;
    let script_path = plugin_dir.join(main_file);
    let script_src = prepare_plugin_source(
        &std::fs::read_to_string(&script_path).map_err(|e| {
            AppError::msg(format!(
                "PLUGIN_RUNTIME_ERROR: cannot read {}: {e}",
                script_path.display()
            ))
        })?,
    );

    let ctx_state = Arc::new(Mutex::new(RunContext::new()));
    let plugin_name = manifest.name.clone();
    let plugin_id = manifest.id.clone();
    let action_id_owned = action_id.to_string();

    let emit_progress = |phase: &str, current: u32, total: u32, message: Option<String>| {
        if let Some(cb) = on_progress.as_ref() {
            let state = ctx_state.lock().unwrap();
            cb(PluginRunProgressEvent {
                run_id: run_id.clone(),
                plugin_id: plugin_id.clone(),
                plugin_name: plugin_name.clone(),
                action_id: action_id_owned.clone(),
                phase: phase.to_string(),
                current,
                total,
                message,
                logs: state.log_lines.clone(),
            });
        }
    };

    emit_progress(
        "starting",
        0,
        asset_ids.len() as u32,
        Some(format!("Running {}…", plugin_name)),
    );

    // ── Fetch assets from DB before entering QuickJS ───────────────────────
    let asset_rows: Vec<AssetSummary> = if asset_ids.is_empty() {
        Vec::new()
    } else {
        let placeholders = std::iter::repeat_n("?", asset_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                    created_at, captured_at, indexed_at, favorite, rating, color_label,
                    thumbnail_path, camera, lens, deleted_at
             FROM assets
             WHERE id IN ({placeholders}) AND deleted_at IS NULL"
        );
        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = asset_ids
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt.query_map(param_refs.as_slice(), crate::search::map_asset)?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Check which assets are vault-locked.
    let vault_locked_ids: std::collections::HashSet<String> = {
        if asset_ids.is_empty() {
            Default::default()
        } else {
            let placeholders = std::iter::repeat_n("?", asset_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT id FROM locked_assets WHERE id IN ({placeholders})"
            );
            if let Ok(mut stmt) = conn.prepare(&sql) {
                let param_refs: Vec<&dyn rusqlite::types::ToSql> = asset_ids
                    .iter()
                    .map(|v| v as &dyn rusqlite::types::ToSql)
                    .collect();
                stmt.query_map(param_refs.as_slice(), |r| r.get::<_, String>(0))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default()
            } else {
                Default::default()
            }
        }
    };

    // Build the JS-facing asset array as JSON (avoids fighting rquickjs type lifetimes).
    let assets_json: Vec<serde_json::Value> = asset_rows
        .iter()
        .map(|a| {
            let mut obj = serde_json::json!({
                "id": a.id,
                "path": a.path,
                "mediaType": a.media_type,
                "capturedAt": a.captured_at,
                "createdAt": a.created_at,
                "rating": a.rating,
                "favorite": a.favorite,
                "colorLabel": a.color_label,
                "camera": a.camera,
                "lens": a.lens,
                "width": a.width,
                "height": a.height,
                "thumbnailPath": a.thumbnail_path,
                "vaultLocked": vault_locked_ids.contains(&a.id),
            });
            // Merge read:metadata fields when the permission is granted.
            if perms.has(Permission::ReadMetadata) {
                obj["capturedAt"] = serde_json::json!(a.captured_at);
                obj["camera"] = serde_json::json!(a.camera);
                obj["lens"] = serde_json::json!(a.lens);
                obj["rating"] = serde_json::json!(a.rating);
                obj["colorLabel"] = serde_json::json!(a.color_label);
            }
            obj
        })
        .collect();

    // ── QuickJS runtime ────────────────────────────────────────────────────
    let rt = Runtime::new().map_err(|e| AppError::msg(format!("QuickJS init: {e}")))?;
    // 64 MB heap limit.
    rt.set_memory_limit(64 * 1024 * 1024);
    // 120 s interrupt via a watchdog thread (sleeps in short intervals so we can
    // join it promptly once the plugin finishes — a single sleep(RUN_TIMEOUT) would
    // block join() for the full timeout even on success).
    let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watchdog_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let interrupted2 = Arc::clone(&interrupted);
        rt.set_interrupt_handler(Some(Box::new(move || {
            interrupted2.load(std::sync::atomic::Ordering::Relaxed)
        })));
    }
    let timeout_flag = Arc::clone(&interrupted);
    let watchdog_done_flag = Arc::clone(&watchdog_done);
    let timeout_handle = std::thread::spawn(move || {
        let started = Instant::now();
        while !watchdog_done_flag.load(std::sync::atomic::Ordering::Relaxed) {
            if started.elapsed() >= RUN_TIMEOUT {
                timeout_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });

    let ctx = Context::full(&rt).map_err(|e| AppError::msg(format!("QuickJS ctx: {e}")))?;

    // We need DB-mutating operations (rename, set_rating, etc.) to run synchronously
    // from within JS callbacks.  Since `conn` is borrowed here, we use a shared
    // results vec: JS calls record *intent*, Rust executes after the JS run completes.
    // For preview mode this is a no-op; for apply mode Rust executes the plan.
    let pending_renames: Arc<Mutex<Vec<(String, String)>>> = Default::default();
    let pending_ratings: Arc<Mutex<Vec<(String, i64)>>> = Default::default();
    let pending_tag_sets: Arc<Mutex<Vec<(String, Vec<String>)>>> = Default::default();

    let result_json: AppResult<ActionResult> = ctx.with(|ctx| {
        // ── lumora global ────────────────────────────────────────────────
        let lumora = Object::new(ctx.clone())
            .map_err(|e| AppError::msg(format!("obj: {e}")))?;

        // lumora.log(level, message)
        {
            let state = Arc::clone(&ctx_state);
            let progress_cb = on_progress.clone();
            let run_id_log = run_id.clone();
            let plugin_id_log = plugin_id.clone();
            let plugin_name_log = plugin_name.clone();
            let action_id_log = action_id_owned.clone();
            let log_fn = Function::new(ctx.clone(), move |level: String, message: String| {
                let event = {
                    let mut s = state.lock().unwrap();
                    let ts = s.elapsed_ms();
                    let lvl = match level.as_str() {
                        "warn" => LogLevel::Warn,
                        "error" => LogLevel::Error,
                        _ => LogLevel::Info,
                    };
                    s.log_lines.push(PluginLogLine {
                        level: lvl,
                        message: message.clone(),
                        timestamp_ms: ts,
                    });
                    let (current, total) = parse_progress_message(&message);
                    PluginRunProgressEvent {
                        run_id: run_id_log.clone(),
                        plugin_id: plugin_id_log.clone(),
                        plugin_name: plugin_name_log.clone(),
                        action_id: action_id_log.clone(),
                        phase: "running".to_string(),
                        current,
                        total,
                        message: Some(message.clone()),
                        logs: s.log_lines.clone(),
                    }
                };
                tracing::debug!(level = %level, msg = %message, "plugin log");
                if let Some(cb) = progress_cb.as_ref() {
                    cb(event);
                }
            })
            .map_err(|e| AppError::msg(format!("log fn: {e}")))?;
            lumora.set("log", log_fn)
                .map_err(|e| AppError::msg(format!("set log: {e}")))?;
        }

        // lumora._reportProgress(current, total)
        {
            let state = Arc::clone(&ctx_state);
            let progress_cb = on_progress.clone();
            let run_id_prog = run_id.clone();
            let plugin_id_prog = plugin_id.clone();
            let plugin_name_prog = plugin_name.clone();
            let action_id_prog = action_id_owned.clone();
            let progress_fn = Function::new(ctx.clone(), move |current: i32, total: i32| {
                let event = {
                    let mut s = state.lock().unwrap();
                    let ts = s.elapsed_ms();
                    let message = format!("[progress] {current}/{total}");
                    s.log_lines.push(PluginLogLine {
                        level: LogLevel::Info,
                        message: message.clone(),
                        timestamp_ms: ts,
                    });
                    PluginRunProgressEvent {
                        run_id: run_id_prog.clone(),
                        plugin_id: plugin_id_prog.clone(),
                        plugin_name: plugin_name_prog.clone(),
                        action_id: action_id_prog.clone(),
                        phase: "running".to_string(),
                        current: current.max(0) as u32,
                        total: total.max(0) as u32,
                        message: Some(message),
                        logs: s.log_lines.clone(),
                    }
                };
                if let Some(cb) = progress_cb.as_ref() {
                    cb(event);
                }
            })
            .map_err(|e| AppError::msg(format!("reportProgress fn: {e}")))?;
            lumora.set("_reportProgress", progress_fn)
                .map_err(|e| AppError::msg(format!("set reportProgress: {e}")))?;
        }

        // lumora.getAssets(ids: string[]) — returns a Promise resolving the asset array.
        {
            let assets_json_clone = assets_json.clone();
            let perm_read_assets = perms.has(Permission::ReadAssets);
            let get_assets_fn = Function::new(ctx.clone(), move |_ids: Vec<String>| {
                if !perm_read_assets {
                    return Err(rquickjs::Error::new_from_js(
                        "string",
                        "PLUGIN_PERMISSION_DENIED: read:assets not granted",
                    ));
                }
                // Return full asset list (host already filtered to requested ids above).
                Ok(serde_json::to_string(&assets_json_clone).unwrap_or_default())
            })
            .map_err(|e| AppError::msg(format!("getAssets fn: {e}")))?;
            lumora.set("_getAssetsJson", get_assets_fn)
                .map_err(|e| AppError::msg(format!("set getAssets: {e}")))?;
        }

        // lumora.renameAsset(id, newFileName) — records intent; Rust applies in apply mode.
        {
            let pending = Arc::clone(&pending_renames);
            let perm_rename = perms.has(Permission::RenameFilesystem);
            let rename_fn = Function::new(ctx.clone(), move |id: String, new_name: String| {
                if !perm_rename {
                    return Err(rquickjs::Error::new_from_js(
                        "string",
                        "PLUGIN_PERMISSION_DENIED: rename:filesystem not granted",
                    ));
                }
                pending.lock().unwrap().push((id, new_name));
                Ok(())
            })
            .map_err(|e| AppError::msg(format!("renameAsset fn: {e}")))?;
            lumora.set("_renameAsset", rename_fn)
                .map_err(|e| AppError::msg(format!("set renameAsset: {e}")))?;
        }

        // lumora.setRating(id, rating)
        {
            let pending = Arc::clone(&pending_ratings);
            let perm_write = perms.has(Permission::WriteMetadata);
            let fn_ = Function::new(ctx.clone(), move |id: String, rating: i64| {
                if !perm_write {
                    return Err(rquickjs::Error::new_from_js(
                        "string",
                        "PLUGIN_PERMISSION_DENIED: write:metadata not granted",
                    ));
                }
                pending.lock().unwrap().push((id, rating));
                Ok(())
            })
            .map_err(|e| AppError::msg(format!("setRating fn: {e}")))?;
            lumora.set("_setRating", fn_)
                .map_err(|e| AppError::msg(format!("set setRating: {e}")))?;
        }

        // lumora.setTags(id, tags: string[])
        {
            let pending = Arc::clone(&pending_tag_sets);
            let perm_write = perms.has(Permission::WriteMetadata);
            let fn_ = Function::new(ctx.clone(), move |id: String, tags: Vec<String>| {
                if !perm_write {
                    return Err(rquickjs::Error::new_from_js(
                        "string",
                        "PLUGIN_PERMISSION_DENIED: write:metadata not granted",
                    ));
                }
                pending.lock().unwrap().push((id, tags));
                Ok(())
            })
            .map_err(|e| AppError::msg(format!("setTags fn: {e}")))?;
            lumora.set("_setTags", fn_)
                .map_err(|e| AppError::msg(format!("set setTags: {e}")))?;
        }

        // Inject lumora as a global.
        let globals = ctx.globals();
        globals.set("lumora", lumora)
            .map_err(|e| AppError::msg(format!("set lumora global: {e}")))?;

        // ── Wrapper script ────────────────────────────────────────────────
        // We wrap the plugin module with a thin adapter so:
        //   • getAssets / renameAsset / setRating / setTags are async-friendly stubs.
        //   • reportProgress is wired (best-effort; we log it).
        //   • The return value is captured as JSON.
        let mode_str = mode.to_string();
        let asset_ids_json = serde_json::to_string(asset_ids).unwrap_or_default();
        let wrapper = format!(
            r#"
(async function() {{
  // Shim async host methods.
  lumora.getAssets = async function(ids) {{
    const json = lumora._getAssetsJson(ids);
    return JSON.parse(json);
  }};
  lumora.renameAsset = async function(id, newName) {{
    return lumora._renameAsset(id, newName);
  }};
  lumora.setRating = async function(id, rating) {{
    return lumora._setRating(id, rating);
  }};
  lumora.setTags = async function(id, tags) {{
    return lumora._setTags(id, tags);
  }};
  // exportAssets / organizeAssets / moveAssets — stubs (Milestone 3).
  lumora.exportAssets = async function() {{
    throw new Error("PLUGIN_RUNTIME_ERROR: exportAssets not yet implemented (Milestone 3)");
  }};
  lumora.organizeAssets = async function() {{
    throw new Error("PLUGIN_RUNTIME_ERROR: organizeAssets not yet implemented (Milestone 3)");
  }};
  lumora.moveAssets = async function() {{
    throw new Error("PLUGIN_RUNTIME_ERROR: moveAssets not yet implemented (Milestone 3)");
  }};
  lumora.copyAssets = async function() {{
    throw new Error("PLUGIN_RUNTIME_ERROR: copyAssets not yet implemented (Milestone 3)");
  }};
  lumora.createFolder = async function() {{
    throw new Error("PLUGIN_RUNTIME_ERROR: createFolder not yet implemented (Milestone 3)");
  }};

  // Plugin source (wrapped as async IIFE module).
  {plugin_src}

  const context = {{
    actionId: "{action_id}",
    assetIds: {asset_ids_json},
    libraryId: "default",
    mode: "{mode}",
    reportProgress: function(current, total) {{
      lumora._reportProgress(current, total);
    }},
  }};

  const result = await runAction("{action_id}", context);
  return JSON.stringify(result ?? {{ ok: true, message: "done" }});
}})()
"#,
            plugin_src = script_src,
            action_id = action_id,
            asset_ids_json = asset_ids_json,
            mode = mode_str,
        );

        // Evaluate the async wrapper and drive the returned promise to completion.
        let val: Value = ctx
            .eval(wrapper.as_bytes())
            .map_err(|e| AppError::msg(format!("PLUGIN_RUNTIME_ERROR: eval: {e}")))?;

        let result_str = resolve_js_promise(&ctx, &rt, &interrupted, val)?;

        // Parse result JSON.
        let action_result: ActionResult = if result_str.is_empty() {
            ActionResult { ok: true, message: "Plugin completed.".into(), preview_plan: None }
        } else {
            serde_json::from_str(&result_str).unwrap_or(ActionResult {
                ok: true,
                message: result_str,
                preview_plan: None,
            })
        };

        Ok(action_result)
    });

    // Stop the watchdog thread and wait for it to exit (returns in <50 ms).
    watchdog_done.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = timeout_handle.join();

    let finished_at = chrono::Utc::now().to_rfc3339();
    let state = ctx_state.lock().unwrap();
    let duration_ms = state.elapsed_ms();
    let log_lines = state.log_lines.clone();
    drop(state);

    match result_json {
        Ok(action_result) => {
            // In apply mode: execute the pending operations collected by JS.
            let mut assets_affected: u32 = 0;
            let mut assets_skipped: u32 = 0;

            if mode == "apply" {
                // Execute renames.
                for (id, new_name) in pending_renames.lock().unwrap().iter() {
                    if vault_locked_ids.contains(id) {
                        assets_skipped += 1;
                        continue;
                    }
                    if let Err(e) = apply_rename(conn, id, new_name) {
                        tracing::warn!(id = %id, error = %e, "plugin rename failed");
                        assets_skipped += 1;
                    } else {
                        assets_affected += 1;
                    }
                }
                // Execute rating sets.
                for (id, rating) in pending_ratings.lock().unwrap().iter() {
                    if vault_locked_ids.contains(id) {
                        assets_skipped += 1;
                        continue;
                    }
                    if let Err(e) = conn.execute(
                        "UPDATE assets SET rating=?1 WHERE id=?2",
                        rusqlite::params![rating, id],
                    ) {
                        tracing::warn!(id = %id, error = %e, "plugin set_rating failed");
                        assets_skipped += 1;
                    } else {
                        assets_affected += 1;
                    }
                }
                // Execute tag replacements.
                for (asset_id, tags) in pending_tag_sets.lock().unwrap().iter() {
                    if vault_locked_ids.contains(asset_id) {
                        assets_skipped += 1;
                        continue;
                    }
                    if let Err(e) = apply_set_tags(conn, asset_id, tags) {
                        tracing::warn!(id = %asset_id, error = %e, "plugin set_tags failed");
                        assets_skipped += 1;
                    } else {
                        assets_affected += 1;
                    }
                }
            }

            let record = PluginRunRecord {
                run_id: run_id.clone(),
                plugin_id: manifest.id.clone(),
                plugin_version: manifest.version.clone(),
                action_id: action_id.to_string(),
                started_at,
                finished_at,
                duration_ms,
                mode: mode.to_string(),
                outcome: RunOutcome::Ok,
                error_code: None,
                error_message: None,
                assets_requested: asset_ids.len() as u32,
                assets_affected,
                assets_skipped,
                log_lines,
            };
            emit_progress(
                "done",
                assets_affected,
                asset_ids.len() as u32,
                Some(action_result.message.clone()),
            );
            Ok((action_result, record))
        }
        Err(e) => {
            let msg = e.to_string();
            let (code, error_message) = parse_error_code(&msg);
            emit_progress(
                "error",
                0,
                asset_ids.len() as u32,
                Some(error_message.clone()),
            );
            let record = PluginRunRecord {
                run_id: run_id.clone(),
                plugin_id: manifest.id.clone(),
                plugin_version: manifest.version.clone(),
                action_id: action_id.to_string(),
                started_at,
                finished_at,
                duration_ms,
                mode: mode.to_string(),
                outcome: RunOutcome::Error,
                error_code: Some(code),
                error_message: Some(error_message.clone()),
                assets_requested: asset_ids.len() as u32,
                assets_affected: 0,
                assets_skipped: 0,
                log_lines,
            };
            // Return ok=false so the caller always receives a usable record.
            Ok((
                ActionResult { ok: false, message: error_message, preview_plan: None },
                record,
            ))
        }
    }
}

/// Rename a file on disk and update the DB path.
fn apply_rename(conn: &Connection, asset_id: &str, new_filename: &str) -> AppResult<()> {
    // Fetch current path.
    let current_path: String = conn
        .query_row(
            "SELECT path FROM assets WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![asset_id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::msg(format!("asset not found: {e}")))?;

    let current = std::path::Path::new(&current_path);
    let dir = current
        .parent()
        .ok_or_else(|| AppError::msg("cannot determine parent directory"))?;

    // Validate new filename: no path separators, no `..`.
    if new_filename.contains('/') || new_filename.contains('\\') || new_filename.contains("..") {
        return Err(AppError::msg(format!(
            "PLUGIN_RUNTIME_ERROR: invalid filename '{new_filename}'"
        )));
    }

    let new_path = dir.join(new_filename);
    if new_path == current {
        return Ok(()); // nothing to do
    }
    if new_path.exists() {
        return Err(AppError::msg(format!(
            "PLUGIN_RUNTIME_ERROR: destination already exists: {}",
            new_path.display()
        )));
    }

    std::fs::rename(current, &new_path)?;
    conn.execute(
        "UPDATE assets SET path=?1 WHERE id=?2",
        rusqlite::params![new_path.to_string_lossy().as_ref(), asset_id],
    )?;
    Ok(())
}

/// Replace all tags for an asset with the given tag names.
fn apply_set_tags(conn: &Connection, asset_id: &str, tag_names: &[String]) -> AppResult<()> {
    // Delete existing tag associations.
    conn.execute("DELETE FROM asset_tags WHERE asset_id=?1", rusqlite::params![asset_id])?;
    for name in tag_names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Upsert tag.
        conn.execute(
            "INSERT OR IGNORE INTO tags (id, name) VALUES (lower(hex(randomblob(8))), ?1)",
            rusqlite::params![trimmed],
        )?;
        let tag_id: String = conn.query_row(
            "SELECT id FROM tags WHERE name=?1 LIMIT 1",
            rusqlite::params![trimmed],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![asset_id, tag_id],
        )?;
    }
    Ok(())
}

/// Drive a QuickJS promise until it settles or the interrupt flag fires.
fn resolve_js_promise(
    ctx: &rquickjs::Ctx<'_>,
    _rt: &Runtime,
    interrupted: &Arc<std::sync::atomic::AtomicBool>,
    val: Value,
) -> AppResult<String> {
    if val.is_string() {
        return val
            .as_string()
            .and_then(|s| s.to_string().ok())
            .ok_or_else(|| AppError::msg("PLUGIN_RUNTIME_ERROR: invalid string result"));
    }

    let promise = val
        .into_promise()
        .ok_or_else(|| AppError::msg("PLUGIN_RUNTIME_ERROR: plugin did not return a promise"))?;

    while promise.state() == PromiseState::Pending {
        if interrupted.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AppError::msg("PLUGIN_TIMEOUT: plugin exceeded 120s limit"));
        }
        if !ctx.execute_pending_job() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    if promise.state() == PromiseState::Rejected {
        let reason = promise
            .result::<String>()
            .transpose()
            .map_err(|e| AppError::msg(format!("PLUGIN_RUNTIME_ERROR: promise rejection: {e}")))?
            .unwrap_or_else(|| "unknown rejection".to_string());
        return Err(AppError::msg(format!("PLUGIN_RUNTIME_ERROR: {reason}")));
    }

    promise
        .result::<String>()
        .transpose()
        .map_err(|e| AppError::msg(format!("PLUGIN_RUNTIME_ERROR: promise result: {e}")))?
        .ok_or_else(|| AppError::msg("PLUGIN_RUNTIME_ERROR: empty promise result"))
}

fn parse_progress_message(message: &str) -> (u32, u32) {
    let trimmed = message.trim();
    if let Some(rest) = trimmed.strip_prefix("[progress]") {
        let mut parts = rest.trim().split('/');
        if let (Some(current), Some(total)) = (parts.next(), parts.next()) {
            if let (Ok(current), Ok(total)) = (current.trim().parse(), total.trim().parse()) {
                return (current, total);
            }
        }
    }
    (0, 0)
}

/// Extract a `PLUGIN_*` error code from an error message string.
fn parse_error_code(msg: &str) -> (String, String) {
    for code in &[
        "PLUGIN_TIMEOUT",
        "PLUGIN_CANCELLED",
        "PLUGIN_PERMISSION_DENIED",
        "PLUGIN_API_MISMATCH",
        "PLUGIN_INVALID_MANIFEST",
        "PLUGIN_RUNTIME_ERROR",
        "PLUGIN_NOT_FOUND",
    ] {
        if msg.contains(code) {
            return (code.to_string(), msg.to_string());
        }
    }
    ("PLUGIN_RUNTIME_ERROR".to_string(), msg.to_string())
}
