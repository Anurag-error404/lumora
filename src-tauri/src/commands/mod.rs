use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::albums;
use crate::blur;
use crate::captions;
use crate::diagnostics;
use crate::duplicates;
use crate::edit::{self, EditOps, EditResult, EditRevisionSummary, SaveMode, SavedEditOps};
use crate::error::{AppError, AppResult};
use crate::export;
use crate::faces;
use crate::history::{self, HistoryAction};
use crate::indexer;
use crate::indexer::queue::IndexerQueue;
use crate::memories;
use crate::ml;
use crate::models::*;
use crate::ocr;
use crate::places;
use crate::preferences::{self, Preferences, StorageSummary};
use crate::prefs_runtime;
use crate::saved_searches;
use crate::search;
use crate::semantic;
use crate::smart;
use crate::state::{open_db, AppState, VaultSession};
use crate::tags;
use crate::thumbnails;
use crate::trash;
use crate::plugins;
use crate::vault;
use crate::views;
use crate::watcher;

#[tauri::command]
pub fn get_library_stats(state: State<'_, AppState>) -> AppResult<LibraryStats> {
    state.with_db(|conn| {
        Ok(LibraryStats {
            total_assets: scalar(conn, "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL")?,
            total_images: scalar(
                conn,
                "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type='image'",
            )?,
            total_videos: scalar(
                conn,
                "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND media_type='video'",
            )?,
            favorites: scalar(
                conn,
                "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL AND favorite=1",
            )?,
            in_trash: scalar(
                conn,
                "SELECT COUNT(*) FROM assets WHERE deleted_at IS NOT NULL",
            )?,
            album_count: scalar(conn, "SELECT COUNT(*) FROM albums")?,
            tag_count: scalar(conn, "SELECT COUNT(*) FROM tags")?,
            watched_folders: scalar(conn, "SELECT COUNT(*) FROM watched_folders")?,
            trash_retention_days: trash::DEFAULT_RETENTION_DAYS,
        })
    })
}

fn scalar(conn: &Connection, sql: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

fn push_history(
    state: &AppState,
    kind: &str,
    label: impl Into<String>,
    detail: Option<&str>,
    undo: HistoryAction,
    redo: HistoryAction,
) -> AppResult<()> {
    let label = label.into();
    let entry = history::make_entry(kind, label.clone(), undo, redo);
    let entry_id = entry.id.clone();
    state.with_db(|conn| {
        history::record_activity(conn, kind, &label, detail)?;
        // Keep entry id aligned with DB activity when possible — store under same label/time.
        let _ = entry_id;
        Ok(())
    })?;
    state.history.lock().push(entry);
    Ok(())
}

#[tauri::command]
pub async fn import_folder(app: AppHandle, path: String) -> AppResult<ImportResult> {
    import_paths(app, vec![path]).await
}

#[tauri::command]
pub async fn import_paths(app: AppHandle, paths: Vec<String>) -> AppResult<ImportResult> {
    if paths.is_empty() {
        return Err(AppError::msg(
            "select at least one file or folder to import",
        ));
    }

    let roots: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    for root in &roots {
        if !root.exists() {
            return Err(AppError::msg(format!("path not found: {}", root.display())));
        }
        if root.is_file() && !indexer::is_supported_media(root) {
            return Err(AppError::msg(format!(
                "unsupported media file: {}",
                root.display()
            )));
        }
        if !root.is_file() && !root.is_dir() {
            return Err(AppError::msg(format!(
                "not a file or folder: {}",
                root.display()
            )));
        }
    }

    // Extract owned paths so we don't hold `State<'_>` across threads.
    let (db_path, thumbs, cancel, app_data) = {
        let state = app.state::<AppState>();
        state
            .import_cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        (
            state.paths.db_path.clone(),
            state.paths.thumbs_dir.clone(),
            Arc::clone(&state.import_cancel),
            state.paths.app_data.clone(),
        )
    };

    let skip_content_dupes = preferences::load(&app_data)
        .map(|p| p.import_export.skip_duplicates)
        .unwrap_or(true);
    let import_options = preferences::load(&app_data)
        .map(|p| indexer::ImportOptions::from_prefs(&p))
        .unwrap_or_default();

    let label = if roots.len() == 1 {
        roots[0].display().to_string()
    } else {
        format!("{} items", roots.len())
    };
    let _ = app.emit(
        "import-progress",
        ImportProgressEvent {
            current: 0,
            total: 0,
            path: label.clone(),
            phase: "scanning".into(),
        },
    );

    // Run the heavy scan/hash/thumbnail work off the main thread so the UI
    // stays responsive and progress events can render live.
    let app_for_job = app.clone();
    let roots_for_job = roots.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> AppResult<ImportResult> {
        let conn = open_db(&db_path)?;

        let result = indexer::import_paths_with_progress(
            &conn,
            &roots_for_job,
            &thumbs,
            cancel,
            skip_content_dupes,
            &import_options,
            |current, total, file| {
                let _ = app_for_job.emit(
                    "import-progress",
                    ImportProgressEvent {
                        current,
                        total,
                        path: file.display().to_string(),
                        phase: "indexing".into(),
                    },
                );
            },
        )?;

        // Only watch directories — individual files don't create a watched root.
        if !result.cancelled {
            for root in &roots_for_job {
                if root.is_dir() {
                    let _ = watcher::add_watched(&conn, root);
                }
            }
        }
        Ok(result)
    })
    .await
    .map_err(|e| AppError::msg(format!("import task failed: {e}")))??;

    // Local-only import analytics (Activity + import_runs). Never sent off-device.
    if let Some(state) = app.try_state::<AppState>() {
        let roots_for_log = roots.clone();
        let result_for_log = result.clone();
        let _ =
            state.with_db(|conn| history::record_import_run(conn, &result_for_log, &roots_for_log));
    }

    if !result.cancelled {
        if let Some(ws) = app.try_state::<Arc<watcher::WatcherService>>() {
            for root in &roots {
                if root.is_dir() {
                    ws.add_root(root.clone());
                }
            }
        }
    }

    // Newly imported photos may need CLIP embeddings / OCR / faces / places / tags.
    if let Some(state) = app.try_state::<AppState>() {
        state.embedder.kick();
        state.ocr.kick();
        state.faces.kick();
        state.places.kick();
        state.tags.kick();
    }

    let phase = if result.cancelled {
        "cancelled"
    } else {
        "done"
    };
    let _ = app.emit(
        "import-progress",
        ImportProgressEvent {
            current: result.scanned,
            total: result.scanned,
            path: label,
            phase: phase.into(),
        },
    );

    tracing::info!(?result, paths = ?paths, "media imported");
    Ok(result)
}

/// Abort an in-flight import. Already-indexed files stay in the library.
#[tauri::command]
pub fn cancel_import(state: State<'_, AppState>) -> AppResult<()> {
    state
        .import_cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn list_assets(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| search::list_assets(conn, limit.min(500), offset, false))
}

#[tauri::command]
pub fn search_assets(
    state: State<'_, AppState>,
    query: String,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| search::search_assets(conn, &query, limit.min(500), offset))
}

#[tauri::command]
pub fn get_index_progress(state: State<'_, AppState>) -> AppResult<IndexProgress> {
    Ok(state.indexer.progress())
}

#[tauri::command]
pub fn get_developer_info(state: State<'_, AppState>) -> AppResult<DeveloperInfo> {
    let thumbnail_stats = diagnostics::directory_stats(&state.paths.thumbs_dir);
    let log_stats = diagnostics::directory_stats(&state.paths.logs_dir);
    let recent_logs = diagnostics::latest_log_lines(&state.paths.logs_dir, 500)?;
    let crash_logs = diagnostics::error_log_lines(&recent_logs, 100);
    let database_size_bytes = std::fs::metadata(&state.paths.db_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let (schema_version, watched_folder_count, activity_count, export_count, import_run_count) =
        state.with_db(|conn| {
            Ok((
                scalar(
                    conn,
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                )?,
                scalar(conn, "SELECT COUNT(*) FROM watched_folders")?,
                scalar(conn, "SELECT COUNT(*) FROM activity_log")?,
                scalar(conn, "SELECT COUNT(*) FROM exports")?,
                scalar(conn, "SELECT COUNT(*) FROM import_runs")?,
            ))
        })?;

    Ok(DeveloperInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        build_profile: if cfg!(debug_assertions) {
            "development".to_string()
        } else {
            "release".to_string()
        },
        debug_build: cfg!(debug_assertions),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_data_path: state.paths.app_data.display().to_string(),
        database_path: state.paths.db_path.display().to_string(),
        database_size_bytes,
        schema_version,
        thumbnails_path: state.paths.thumbs_dir.display().to_string(),
        thumbnail_count: thumbnail_stats.file_count,
        thumbnail_size_bytes: thumbnail_stats.size_bytes,
        logs_path: state.paths.logs_dir.display().to_string(),
        log_file_count: log_stats.file_count,
        log_size_bytes: log_stats.size_bytes,
        watched_folder_count,
        activity_count,
        export_count,
        import_run_count,
        ffmpeg_available: thumbnails::ffmpeg::ffmpeg_available(),
        index_progress: state.indexer.progress(),
        recent_logs,
        crash_logs,
    })
}

#[tauri::command]
pub fn get_preferences(state: State<'_, AppState>) -> AppResult<Preferences> {
    preferences::load(&state.paths.app_data)
}

#[tauri::command]
pub fn set_preferences(
    app: AppHandle,
    state: State<'_, AppState>,
    prefs: Preferences,
) -> AppResult<Preferences> {
    preferences::save(&state.paths.app_data, &prefs)?;
    prefs_runtime::touch_user_activity();

    if let Some(ws) = app.try_state::<Arc<watcher::WatcherService>>() {
        ws.set_enabled(prefs.library.watch_folders_enabled);
        if prefs.library.watch_folders_enabled {
            let roots = state
                .with_db(watcher::load_watched_paths)
                .unwrap_or_default();
            ws.set_roots(roots);
        }
    }

    // Resume background workers immediately when preferences allow background work.
    if prefs_runtime::should_run_background(&prefs) {
        state.embedder.kick();
        state.ocr.kick();
        state.faces.kick();
        state.places.kick();
        state.tags.kick();
    }

    Ok(prefs)
}

/// Record foreground UI activity for idle-only background processing.
#[tauri::command]
pub fn ping_user_activity() {
    prefs_runtime::touch_user_activity();
}

#[tauri::command]
pub fn get_storage_summary(state: State<'_, AppState>) -> AppResult<StorageSummary> {
    state.with_db(|conn| {
        preferences::storage_summary(
            conn,
            &state.paths.app_data,
            &state.paths.db_path,
            &state.paths.thumbs_dir,
            &state.paths.models_dir,
            &state.paths.logs_dir,
        )
    })
}

/// Delete thumbnail cache files. Library entries stay intact; previews regenerate
/// on demand / via rebuild.
#[tauri::command]
pub fn clear_thumbnail_cache(state: State<'_, AppState>) -> AppResult<u64> {
    Ok(clear_thumbs_dir(&state.paths.thumbs_dir))
}

/// Clear the thumbnail cache, then regenerate missing previews for library images.
#[tauri::command]
pub fn rebuild_thumbnail_cache(state: State<'_, AppState>) -> AppResult<u32> {
    let thumbs = state.paths.thumbs_dir.clone();
    let _ = clear_thumbs_dir(&thumbs);
    state.with_db(|conn| thumbnails::repair_missing_thumbnails(conn, &thumbs))
}

#[tauri::command]
pub fn optimize_database(state: State<'_, AppState>) -> AppResult<()> {
    state.with_db(|conn| {
        conn.execute_batch("PRAGMA optimize; VACUUM;")?;
        Ok(())
    })
}

fn clear_thumbs_dir(thumbs: &std::path::Path) -> u64 {
    let mut removed = 0u64;
    if !thumbs.is_dir() {
        return 0;
    }
    for entry in walkdir::WalkDir::new(thumbs)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[tauri::command]
pub fn set_favorite(state: State<'_, AppState>, id: String, favorite: bool) -> AppResult<()> {
    state.with_db(|conn| {
        let changed = conn.execute(
            "UPDATE assets SET favorite=?1 WHERE id=?2",
            params![if favorite { 1 } else { 0 }, id],
        )?;
        if changed == 0 {
            return Err(AppError::msg("asset not found"));
        }
        Ok(())
    })?;
    let label = if favorite {
        "Added 1 photo to favourites"
    } else {
        "Removed 1 photo from favourites"
    };
    push_history(
        &state,
        "favorite",
        label,
        None,
        HistoryAction::SetFavorites {
            asset_ids: vec![id.clone()],
            favorite: !favorite,
        },
        HistoryAction::SetFavorites {
            asset_ids: vec![id],
            favorite,
        },
    )?;
    Ok(())
}

#[tauri::command]
pub fn set_favorites(
    state: State<'_, AppState>,
    ids: Vec<String>,
    favorite: bool,
) -> AppResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let count = state.with_db(|conn| {
        let mut count = 0usize;
        let value = if favorite { 1 } else { 0 };
        for id in &ids {
            count += conn.execute(
                "UPDATE assets SET favorite=?1 WHERE id=?2",
                params![value, id],
            )?;
        }
        Ok(count)
    })?;
    let label = if favorite {
        format!("Added {count} photo(s) to favourites")
    } else {
        format!("Removed {count} photo(s) from favourites")
    };
    push_history(
        &state,
        "favorite",
        label,
        None,
        HistoryAction::SetFavorites {
            asset_ids: ids.clone(),
            favorite: !favorite,
        },
        HistoryAction::SetFavorites {
            asset_ids: ids,
            favorite,
        },
    )?;
    Ok(count)
}

#[tauri::command]
pub fn set_rating(state: State<'_, AppState>, id: String, rating: i64) -> AppResult<()> {
    if !(0..=5).contains(&rating) {
        return Err(AppError::msg("rating must be 0-5"));
    }
    state.with_db(|conn| {
        conn.execute(
            "UPDATE assets SET rating=?1 WHERE id=?2",
            params![rating, id],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn set_color_label(
    state: State<'_, AppState>,
    id: String,
    color_label: Option<String>,
) -> AppResult<()> {
    state.with_db(|conn| {
        conn.execute(
            "UPDATE assets SET color_label=?1 WHERE id=?2",
            params![color_label, id],
        )?;
        Ok(())
    })
}

/// Bulk-rate assets (0 clears). Records an undoable history entry that
/// restores each asset's previous rating.
#[tauri::command]
pub fn set_ratings(state: State<'_, AppState>, ids: Vec<String>, rating: i64) -> AppResult<usize> {
    if !(0..=5).contains(&rating) {
        return Err(AppError::msg("rating must be 0-5"));
    }
    if ids.is_empty() {
        return Ok(0);
    }
    let (asset_ids, previous) = state.with_db(|conn| {
        let mut asset_ids = Vec::new();
        let mut previous = Vec::new();
        for id in &ids {
            let prev: Option<i64> = conn
                .query_row("SELECT rating FROM assets WHERE id=?1", params![id], |r| {
                    r.get(0)
                })
                .ok();
            let Some(prev) = prev else { continue };
            conn.execute(
                "UPDATE assets SET rating=?1 WHERE id=?2",
                params![rating, id],
            )?;
            asset_ids.push(id.clone());
            previous.push(prev);
        }
        Ok((asset_ids, previous))
    })?;
    let count = asset_ids.len();
    if count == 0 {
        return Ok(0);
    }
    let label = if rating == 0 {
        format!("Cleared rating on {count} photo(s)")
    } else {
        format!("Rated {count} photo(s) {rating} star(s)")
    };
    push_history(
        &state,
        "rating",
        label,
        None,
        HistoryAction::SetRatings {
            asset_ids: asset_ids.clone(),
            ratings: previous,
        },
        HistoryAction::SetRatings {
            asset_ids: asset_ids.clone(),
            ratings: vec![rating; count],
        },
    )?;
    Ok(count)
}

/// Bulk-apply a colour label (None clears). Undo restores each asset's
/// previous label.
#[tauri::command]
pub fn set_color_labels(
    state: State<'_, AppState>,
    ids: Vec<String>,
    color_label: Option<String>,
) -> AppResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let (asset_ids, previous) = state.with_db(|conn| {
        let mut asset_ids = Vec::new();
        let mut previous = Vec::new();
        for id in &ids {
            let prev: Option<Option<String>> = conn
                .query_row(
                    "SELECT color_label FROM assets WHERE id=?1",
                    params![id],
                    |r| r.get(0),
                )
                .ok();
            let Some(prev) = prev else { continue };
            conn.execute(
                "UPDATE assets SET color_label=?1 WHERE id=?2",
                params![color_label, id],
            )?;
            asset_ids.push(id.clone());
            previous.push(prev);
        }
        Ok((asset_ids, previous))
    })?;
    let count = asset_ids.len();
    if count == 0 {
        return Ok(0);
    }
    let label = match &color_label {
        Some(color) => format!("Labelled {count} photo(s) {color}"),
        None => format!("Removed colour label from {count} photo(s)"),
    };
    push_history(
        &state,
        "label",
        label,
        None,
        HistoryAction::SetColorLabels {
            asset_ids: asset_ids.clone(),
            labels: previous,
        },
        HistoryAction::SetColorLabels {
            asset_ids: asset_ids.clone(),
            labels: vec![color_label; count],
        },
    )?;
    Ok(count)
}

#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> AppResult<Vec<Tag>> {
    state.with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, COUNT(a.id)
             FROM tags t
             LEFT JOIN asset_tags at ON at.tag_id = t.id
             LEFT JOIN assets a ON a.id = at.asset_id AND a.deleted_at IS NULL
             GROUP BY t.id
             ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                asset_count: r.get(2)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
}

#[tauri::command]
pub fn list_tag_assets(
    state: State<'_, AppState>,
    tag_id: String,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| search::list_assets_for_tag(conn, &tag_id, limit, offset))
}

/// Rating and colour-label facet counts for the Tags browsing page.
#[tauri::command]
pub fn get_library_facets(state: State<'_, AppState>) -> AppResult<LibraryFacets> {
    state.with_db(|conn| {
        let (ratings, color_labels) = search::facet_counts(conn)?;
        Ok(LibraryFacets {
            ratings,
            color_labels,
        })
    })
}

/// List assets matching any combination of tags, ratings, and colour labels.
#[tauri::command]
pub fn list_tag_browse_assets(
    state: State<'_, AppState>,
    filter: TagBrowseFilter,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| search::list_assets_for_browse_filter(conn, &filter, limit, offset))
}

#[tauri::command]
pub fn create_tag(state: State<'_, AppState>, name: String) -> AppResult<Tag> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("tag name required"));
    }
    state.with_db(|conn| {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tags (id, name) VALUES (?1, ?2)",
            params![id, name],
        )?;
        Ok(Tag {
            id,
            name,
            asset_count: 0,
        })
    })
}

#[tauri::command]
pub fn tag_asset(state: State<'_, AppState>, asset_id: String, tag_id: String) -> AppResult<()> {
    state.with_db(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
            params![asset_id, tag_id],
        )?;
        indexer::refresh_fts(conn, &asset_id)?;
        Ok(())
    })
}

#[tauri::command]
pub fn untag_asset(state: State<'_, AppState>, asset_id: String, tag_id: String) -> AppResult<()> {
    state.with_db(|conn| {
        conn.execute(
            "DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id=?2",
            params![asset_id, tag_id],
        )?;
        indexer::refresh_fts(conn, &asset_id)?;
        Ok(())
    })
}

#[tauri::command]
pub fn list_albums(state: State<'_, AppState>) -> AppResult<Vec<Album>> {
    state.with_db(albums::list_albums)
}

#[tauri::command]
pub fn list_saved_searches(state: State<'_, AppState>) -> AppResult<Vec<SavedSearch>> {
    state.with_db(saved_searches::list)
}

#[tauri::command]
pub fn record_recent_search(state: State<'_, AppState>, query: String) -> AppResult<SavedSearch> {
    state.with_db(|conn| saved_searches::record(conn, &query))
}

#[tauri::command]
pub fn delete_saved_search(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.with_db(|conn| saved_searches::delete(conn, &id))
}

#[tauri::command]
pub fn clear_recent_searches(state: State<'_, AppState>) -> AppResult<usize> {
    state.with_db(saved_searches::clear)
}

/// Albums and tags currently associated with a single asset.
#[tauri::command]
pub fn get_asset_organisation(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<AssetOrganisation> {
    state.with_db(|conn| {
        let mut album_stmt = conn.prepare(
            "SELECT
                a.id,
                a.name,
                a.cover_asset_id,
                a.created_at,
                (
                  SELECT COUNT(*)
                  FROM album_assets aa2
                  JOIN assets x ON x.id = aa2.asset_id AND x.deleted_at IS NULL
                  WHERE aa2.album_id = a.id
                ),
                (
                  SELECT c.thumbnail_path
                  FROM assets c
                  WHERE c.id = a.cover_asset_id AND c.deleted_at IS NULL
                )
             FROM albums a
             JOIN album_assets aa ON aa.album_id = a.id
             WHERE aa.asset_id = ?1
             ORDER BY a.name COLLATE NOCASE",
        )?;
        let albums = album_stmt
            .query_map(params![id], |r| {
                Ok(Album {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    cover_asset_id: r.get(2)?,
                    created_at: r.get(3)?,
                    asset_count: r.get(4)?,
                    cover_thumbnail_path: r.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut tag_stmt = conn.prepare(
            "SELECT
                t.id,
                t.name,
                (
                  SELECT COUNT(*)
                  FROM asset_tags at2
                  JOIN assets x ON x.id = at2.asset_id AND x.deleted_at IS NULL
                  WHERE at2.tag_id = t.id
                )
             FROM tags t
             JOIN asset_tags at ON at.tag_id = t.id
             WHERE at.asset_id = ?1
             ORDER BY t.name COLLATE NOCASE",
        )?;
        let tags = tag_stmt
            .query_map(params![id], |r| {
                Ok(Tag {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    asset_count: r.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(AssetOrganisation { albums, tags })
    })
}

#[tauri::command]
pub fn create_album(state: State<'_, AppState>, name: String) -> AppResult<Album> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("album name required"));
    }
    let album = state.with_db(|conn| {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO albums (id, name, created_at) VALUES (?1, ?2, ?3)",
            params![id, name, created_at],
        )?;
        Ok(Album {
            id,
            name,
            cover_asset_id: None,
            cover_thumbnail_path: None,
            created_at,
            asset_count: 0,
        })
    })?;
    state.with_db(|conn| {
        history::record_activity(
            conn,
            "album",
            &format!("Created album “{}”", album.name),
            Some(&album.id),
        )
    })?;
    Ok(album)
}

#[tauri::command]
pub fn rename_album(state: State<'_, AppState>, id: String, name: String) -> AppResult<()> {
    state.with_db(|conn| {
        conn.execute(
            "UPDATE albums SET name=?1 WHERE id=?2",
            params![name.trim(), id],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn delete_album(
    state: State<'_, AppState>,
    id: String,
    delete_assets: bool,
) -> AppResult<usize> {
    let (album_name, asset_ids): (String, Vec<String>) = state.with_db(|conn| {
        let name: String = conn
            .query_row("SELECT name FROM albums WHERE id=?1", params![id], |r| {
                r.get(0)
            })
            .map_err(|_| AppError::msg("album not found"))?;
        let mut stmt = conn.prepare(
            "SELECT aa.asset_id
             FROM album_assets aa
             JOIN assets a ON a.id = aa.asset_id
             WHERE aa.album_id = ?1 AND a.deleted_at IS NULL",
        )?;
        let ids = stmt
            .query_map(params![id], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok((name, ids))
    })?;

    let mut trashed = 0usize;
    if delete_assets && !asset_ids.is_empty() {
        trashed = state.with_db(|conn| trash::soft_delete(conn, &asset_ids))?;
        push_history(
            &state,
            "trash",
            format!("Moved {trashed} item(s) to trash with album “{album_name}”"),
            Some(&id),
            HistoryAction::Restore {
                asset_ids: asset_ids.clone(),
            },
            HistoryAction::SoftDelete {
                asset_ids: asset_ids.clone(),
            },
        )?;
    }

    state.with_db(|conn| {
        conn.execute("DELETE FROM albums WHERE id=?1", params![id])?;
        history::record_activity(
            conn,
            "album",
            &format!("Deleted album “{album_name}”"),
            Some(&id),
        )?;
        Ok(())
    })?;
    state.history.lock().invalidate_album(&id);
    Ok(trashed)
}

#[tauri::command]
pub fn add_to_album(
    state: State<'_, AppState>,
    album_id: String,
    asset_id: String,
) -> AppResult<()> {
    state.with_db(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO album_assets (album_id, asset_id) VALUES (?1, ?2)",
            params![album_id, asset_id],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn add_assets_to_album(
    state: State<'_, AppState>,
    album_id: String,
    asset_ids: Vec<String>,
) -> AppResult<usize> {
    if asset_ids.is_empty() {
        return Ok(0);
    }
    let album_name: String = state.with_db(|conn| {
        conn.query_row(
            "SELECT name FROM albums WHERE id=?1",
            params![album_id],
            |r| r.get(0),
        )
        .map_err(|_| AppError::msg("album not found"))
    })?;
    let count = state.with_db(|conn| {
        let mut count = 0usize;
        for asset_id in &asset_ids {
            count += conn.execute(
                "INSERT OR IGNORE INTO album_assets (album_id, asset_id) VALUES (?1, ?2)",
                params![album_id, asset_id],
            )?;
        }
        if let Some(first) = asset_ids.first() {
            albums::ensure_cover(conn, &album_id, first)?;
        }
        Ok(count)
    })?;
    if count > 0 {
        let detail = album_id.clone();
        push_history(
            &state,
            "album",
            format!("Added {count} photo(s) to “{album_name}”"),
            Some(&detail),
            HistoryAction::RemoveFromAlbum {
                album_id: album_id.clone(),
                asset_ids: asset_ids.clone(),
            },
            HistoryAction::AddToAlbum {
                album_id,
                asset_ids,
            },
        )?;
    }
    Ok(count)
}

#[tauri::command]
pub fn create_album_with_assets(
    state: State<'_, AppState>,
    name: String,
    asset_ids: Vec<String>,
) -> AppResult<Album> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("album name required"));
    }
    let album = state.with_db(|conn| {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let cover = asset_ids.first().cloned();
        conn.execute(
            "INSERT INTO albums (id, name, cover_asset_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, cover, created_at],
        )?;
        let mut added = 0i64;
        for asset_id in &asset_ids {
            added += conn.execute(
                "INSERT OR IGNORE INTO album_assets (album_id, asset_id) VALUES (?1, ?2)",
                params![id, asset_id],
            )? as i64;
        }
        let cover_thumbnail_path: Option<String> = cover.as_ref().and_then(|cid| {
            conn.query_row(
                "SELECT thumbnail_path FROM assets WHERE id = ?1",
                params![cid],
                |r| r.get(0),
            )
            .ok()
            .flatten()
        });
        Ok(Album {
            id,
            name,
            cover_asset_id: cover,
            cover_thumbnail_path,
            created_at,
            asset_count: added,
        })
    })?;
    if !asset_ids.is_empty() {
        let count = asset_ids.len();
        push_history(
            &state,
            "album",
            format!("Created album “{}” with {count} photo(s)", album.name),
            Some(&album.id),
            HistoryAction::RemoveFromAlbum {
                album_id: album.id.clone(),
                asset_ids: asset_ids.clone(),
            },
            HistoryAction::AddToAlbum {
                album_id: album.id.clone(),
                asset_ids,
            },
        )?;
    } else {
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "album",
                &format!("Created album “{}”", album.name),
                Some(&album.id),
            )
        })?;
    }
    Ok(album)
}

#[tauri::command]
pub fn tag_assets(
    state: State<'_, AppState>,
    tag_id: String,
    asset_ids: Vec<String>,
) -> AppResult<usize> {
    state.with_db(|conn| {
        let mut count = 0usize;
        for asset_id in &asset_ids {
            count += conn.execute(
                "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
                params![asset_id, tag_id],
            )?;
            indexer::refresh_fts(conn, asset_id)?;
        }
        Ok(count)
    })
}

#[tauri::command]
pub fn create_tag_and_assign(
    state: State<'_, AppState>,
    name: String,
    asset_ids: Vec<String>,
) -> AppResult<Tag> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::msg("tag name required"));
    }
    state.with_db(|conn| {
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )
            .ok();
        let id = if let Some(id) = existing {
            id
        } else {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO tags (id, name) VALUES (?1, ?2)",
                params![id, name],
            )?;
            id
        };
        for asset_id in &asset_ids {
            conn.execute(
                "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
                params![asset_id, id],
            )?;
            indexer::refresh_fts(conn, asset_id)?;
        }
        Ok(Tag {
            id,
            name,
            asset_count: asset_ids.len() as i64,
        })
    })
}

#[tauri::command]
pub fn remove_from_album(
    state: State<'_, AppState>,
    album_id: String,
    asset_id: String,
) -> AppResult<()> {
    state.with_db(|conn| {
        conn.execute(
            "DELETE FROM album_assets WHERE album_id=?1 AND asset_id=?2",
            params![album_id, asset_id],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn list_album_assets(
    state: State<'_, AppState>,
    album_id: String,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT a.id, a.path, a.hash, a.perceptual_hash, a.media_type, a.width, a.height, a.duration_ms,
                    a.created_at, a.captured_at, a.indexed_at, a.favorite, a.rating, a.color_label,
                    a.thumbnail_path, a.camera, a.lens, a.deleted_at
             FROM assets a
             JOIN album_assets aa ON aa.asset_id = a.id
             WHERE aa.album_id = ?1 AND a.deleted_at IS NULL
             ORDER BY COALESCE(a.captured_at, a.created_at) DESC
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![album_id, limit, offset], search::map_asset)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
}

#[tauri::command]
pub fn timeline_months(state: State<'_, AppState>) -> AppResult<Vec<TimelineMonth>> {
    state.with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT CAST(strftime('%Y', COALESCE(captured_at, created_at)) AS INTEGER) AS y,
                    CAST(strftime('%m', COALESCE(captured_at, created_at)) AS INTEGER) AS m,
                    COUNT(*)
             FROM assets
             WHERE deleted_at IS NULL
             GROUP BY y, m
             ORDER BY y DESC, m DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(TimelineMonth {
                year: r.get(0)?,
                month: r.get::<_, i64>(1)? as u32,
                count: r.get(2)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
}

#[tauri::command]
pub fn list_assets_for_month(
    state: State<'_, AppState>,
    year: i32,
    month: u32,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let prefix = format!("{year:04}-{month:02}");
    state.with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                    created_at, captured_at, indexed_at, favorite, rating, color_label,
                    thumbnail_path, camera, lens, deleted_at
             FROM assets
             WHERE deleted_at IS NULL
               AND strftime('%Y-%m', COALESCE(captured_at, created_at)) = ?1
             ORDER BY COALESCE(captured_at, created_at) DESC
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![prefix, limit, offset], search::map_asset)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
}

#[tauri::command]
pub fn list_recent(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                    created_at, captured_at, indexed_at, favorite, rating, color_label,
                    thumbnail_path, camera, lens, deleted_at
             FROM assets
             WHERE deleted_at IS NULL
             ORDER BY indexed_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], search::map_asset)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
}

#[tauri::command]
pub fn list_recently_viewed(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| views::list_recently_viewed(conn, limit, offset))
}

#[tauri::command]
pub fn record_asset_view(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.with_db(|conn| views::record_view(conn, &id))
}

/// Assets belonging to a smart collection ("videos", "rawPhotos", "screenshots").
#[tauri::command]
pub fn list_smart_collection(
    state: State<'_, AppState>,
    kind: String,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let collection = smart::SmartCollection::parse(&kind)?;
    state.with_db(|conn| smart::list(conn, collection, limit, offset))
}

/// Item counts for every smart collection, keyed by collection id.
#[tauri::command]
pub fn smart_collection_counts(state: State<'_, AppState>) -> AppResult<SmartCounts> {
    state.with_db(|conn| {
        let mut counts = SmartCounts::new();
        for kind in smart::SmartCollection::ALL {
            counts.insert(kind.id().to_string(), smart::count(conn, kind)?);
        }
        Ok(counts)
    })
}

/// What models are installed and whether semantic search can run yet.
#[tauri::command]
pub fn ml_status(state: State<'_, AppState>) -> AppResult<ml::MlStatus> {
    let models_dir = state.paths.models_dir.clone();
    state.with_db(|conn| ml::status(conn, &models_dir))
}

/// Download the semantic-search model bundle.
///
/// This is the only place in LUMORA that reaches the network, and it runs only
/// from an explicit user action. Each file is checksum-verified before it is
/// registered; a mismatch discards the download.
#[tauri::command]
pub async fn install_semantic_models(app: AppHandle) -> AppResult<ml::MlStatus> {
    let (db_path, models_dir) = {
        let state = app.state::<AppState>();
        (state.paths.db_path.clone(), state.paths.models_dir.clone())
    };

    let app_for_job = app.clone();
    let dir_for_job = models_dir.clone();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let conn = open_db(&db_path)?;

        let entries: Vec<_> = ml::catalog::bundle(ml::catalog::SEMANTIC_BUNDLE).collect();
        let total_files = entries.len();
        for (index, entry) in entries.into_iter().enumerate() {
            let app_progress = app_for_job.clone();
            let file_label = entry.file_name.to_string();
            ml::download_and_install(&conn, &dir_for_job, entry, move |done, total| {
                let _ = app_progress.emit(
                    "model-progress",
                    ModelProgressEvent {
                        model_id: file_label.clone(),
                        file_index: index as u32 + 1,
                        file_count: total_files as u32,
                        downloaded: done,
                        total,
                    },
                );
            })?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(format!("model install task failed: {e}")))??;

    let state = app.state::<AppState>();
    state.embedder.invalidate();
    state.embedder.kick();
    state.with_db(|conn| ml::status(conn, &models_dir))
}

/// Delete a model file, its registry row, and any vectors it produced.
#[tauri::command]
pub fn remove_ml_model(state: State<'_, AppState>, id: String) -> AppResult<ml::MlStatus> {
    let models_dir = state.paths.models_dir.clone();
    let status = state.with_db(|conn| {
        ml::remove(conn, &models_dir, &id)?;
        ml::status(conn, &models_dir)
    })?;
    state.embedder.invalidate();
    state.ocr.invalidate();
    state.faces.invalidate();
    state.tags.invalidate();
    state.captions.invalidate();
    memories::prose::invalidate_engine();
    Ok(status)
}

/// Drop every embedding without uninstalling models, so they can be rebuilt.
#[tauri::command]
pub fn clear_ml_embeddings(state: State<'_, AppState>) -> AppResult<usize> {
    let app_data = state.paths.app_data.clone();
    let n = state.with_db(ml::clear_embeddings)?;
    semantic::ann::invalidate_and_remove(&app_data);
    state.embedder.kick();
    Ok(n)
}

/// Whether semantic search can run, and how much of the library is indexed.
#[tauri::command]
pub fn semantic_status(state: State<'_, AppState>) -> AppResult<SemanticStatus> {
    state.with_db(|conn| {
        let (embedded, total) = semantic::coverage(conn)?;
        Ok(SemanticStatus {
            model_ready: ml::semantic_ready(conn)?,
            embedded,
            total,
        })
    })
}

/// Live progress of the background embedding worker.
#[tauri::command]
pub fn embed_progress(state: State<'_, AppState>) -> AppResult<semantic::worker::EmbedProgress> {
    state.embedder.progress()
}

/// Ask the embedder to resume (e.g. after import). Harmless if already running.
#[tauri::command]
pub fn kick_embedding(state: State<'_, AppState>) -> AppResult<()> {
    state.embedder.kick();
    Ok(())
}

/// Pause the embedding worker until the next kick/resume.
#[tauri::command]
pub fn pause_embedding(state: State<'_, AppState>) -> AppResult<()> {
    state.embedder.pause();
    Ok(())
}

/// Download the RapidOCR PP-OCRv4 bundle (det + rec + dict).
#[tauri::command]
pub async fn install_ocr_models(app: AppHandle) -> AppResult<ml::MlStatus> {
    let (db_path, models_dir, app_data) = {
        let state = app.state::<AppState>();
        (
            state.paths.db_path.clone(),
            state.paths.models_dir.clone(),
            state.paths.app_data.clone(),
        )
    };

    let app_for_job = app.clone();
    let dir_for_job = models_dir.clone();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let conn = open_db(&db_path)?;

        let entries: Vec<_> = ml::catalog::bundle(ml::catalog::OCR_BUNDLE).collect();
        let total_files = entries.len();
        for (index, entry) in entries.into_iter().enumerate() {
            let app_progress = app_for_job.clone();
            let file_label = entry.file_name.to_string();
            ml::download_and_install(&conn, &dir_for_job, entry, move |done, total| {
                let _ = app_progress.emit(
                    "model-progress",
                    ModelProgressEvent {
                        model_id: file_label.clone(),
                        file_index: index as u32 + 1,
                        file_count: total_files as u32,
                        downloaded: done,
                        total,
                    },
                );
            })?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(format!("OCR install task failed: {e}")))??;

    // Turn OCR on once models are present so the worker actually runs.
    if let Ok(mut prefs) = preferences::load(&app_data) {
        prefs.ai.ocr = true;
        let _ = preferences::save(&app_data, &prefs);
    }

    let state = app.state::<AppState>();
    state.ocr.invalidate();
    state.ocr.kick();
    state.with_db(|conn| ml::status(conn, &models_dir))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrStatusDto {
    pub model_ready: bool,
    pub enabled: bool,
    pub done: i64,
    pub total: i64,
}

#[tauri::command]
pub fn ocr_status(state: State<'_, AppState>) -> AppResult<OcrStatusDto> {
    let prefs = preferences::load(&state.paths.app_data)?;
    state.with_db(|conn| {
        let cov = ocr::coverage(conn)?;
        Ok(OcrStatusDto {
            model_ready: ocr::ocr_ready(conn)?,
            enabled: prefs.ai.ocr,
            done: cov.done,
            total: cov.total,
        })
    })
}

#[tauri::command]
pub fn ocr_progress(state: State<'_, AppState>) -> AppResult<ocr::worker::OcrProgress> {
    state.ocr.progress()
}

#[tauri::command]
pub fn kick_ocr(state: State<'_, AppState>) -> AppResult<()> {
    state.ocr.kick();
    Ok(())
}

#[tauri::command]
pub fn pause_ocr(state: State<'_, AppState>) -> AppResult<()> {
    state.ocr.pause();
    Ok(())
}

#[tauri::command]
pub fn clear_ocr_text(state: State<'_, AppState>) -> AppResult<usize> {
    let n = state.with_db(ocr::clear_all)?;
    state.ocr.kick();
    Ok(n)
}

#[tauri::command]
pub fn get_asset_text(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<Option<ocr::AssetText>> {
    state.with_db(|conn| ocr::get_asset_text(conn, &asset_id))
}

/// Download InsightFace buffalo_l (SCRFD + ArcFace).
#[tauri::command]
pub async fn install_face_models(app: AppHandle) -> AppResult<ml::MlStatus> {
    let (db_path, models_dir, app_data) = {
        let state = app.state::<AppState>();
        (
            state.paths.db_path.clone(),
            state.paths.models_dir.clone(),
            state.paths.app_data.clone(),
        )
    };

    let app_for_job = app.clone();
    let dir_for_job = models_dir.clone();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let conn = open_db(&db_path)?;

        let entries: Vec<_> = ml::catalog::bundle(ml::catalog::FACES_BUNDLE).collect();
        let total_files = entries.len();
        for (index, entry) in entries.into_iter().enumerate() {
            let app_progress = app_for_job.clone();
            let file_label = entry.file_name.to_string();
            ml::download_and_install(&conn, &dir_for_job, entry, move |done, total| {
                let _ = app_progress.emit(
                    "model-progress",
                    ModelProgressEvent {
                        model_id: file_label.clone(),
                        file_index: index as u32 + 1,
                        file_count: total_files as u32,
                        downloaded: done,
                        total,
                    },
                );
            })?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(format!("faces install task failed: {e}")))??;

    if let Ok(mut prefs) = preferences::load(&app_data) {
        prefs.ai.face_recognition = true;
        let _ = preferences::save(&app_data, &prefs);
    }

    let state = app.state::<AppState>();
    state.faces.invalidate();
    state.faces.kick();
    state.with_db(|conn| ml::status(conn, &models_dir))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacesStatusDto {
    pub model_ready: bool,
    pub enabled: bool,
    pub done: i64,
    pub total: i64,
    pub people_count: i64,
}

#[tauri::command]
pub fn faces_status(state: State<'_, AppState>) -> AppResult<FacesStatusDto> {
    let prefs = preferences::load(&state.paths.app_data)?;
    state.with_db(|conn| {
        let cov = faces::coverage(conn)?;
        let people_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM people WHERE face_count > 0",
            [],
            |r| r.get(0),
        )?;
        Ok(FacesStatusDto {
            model_ready: faces::faces_ready(conn)?,
            enabled: prefs.ai.face_recognition,
            done: cov.done,
            total: cov.total,
            people_count,
        })
    })
}

#[tauri::command]
pub fn faces_progress(state: State<'_, AppState>) -> AppResult<faces::worker::FacesProgress> {
    state.faces.progress()
}

#[tauri::command]
pub fn kick_faces(state: State<'_, AppState>) -> AppResult<()> {
    state.faces.kick();
    Ok(())
}

#[tauri::command]
pub fn pause_faces(state: State<'_, AppState>) -> AppResult<()> {
    state.faces.pause();
    Ok(())
}

#[tauri::command]
pub fn clear_face_data(state: State<'_, AppState>) -> AppResult<usize> {
    let faces_dir = state.paths.faces_dir.clone();
    let n = state.with_db(|conn| faces::clear_all(conn, &faces_dir))?;
    state.faces.invalidate();
    state.faces.kick();
    Ok(n)
}

/// Download MobileNetV4 + ImageNet labels for on-device auto-tags.
#[tauri::command]
pub async fn install_tags_models(app: AppHandle) -> AppResult<ml::MlStatus> {
    let (db_path, models_dir, app_data) = {
        let state = app.state::<AppState>();
        (
            state.paths.db_path.clone(),
            state.paths.models_dir.clone(),
            state.paths.app_data.clone(),
        )
    };

    let app_for_job = app.clone();
    let dir_for_job = models_dir.clone();
    let db_path_for_install = db_path.clone();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let conn = open_db(&db_path_for_install)?;

        let entries: Vec<_> = ml::catalog::bundle(ml::catalog::TAGS_BUNDLE).collect();
        let total_files = entries.len();
        for (index, entry) in entries.into_iter().enumerate() {
            let app_progress = app_for_job.clone();
            let file_label = entry.file_name.to_string();
            tracing::info!(
                model = entry.id,
                file = index + 1,
                of = total_files,
                "installing auto-tags bundle file"
            );
            ml::download_and_install(&conn, &dir_for_job, entry, move |done, total| {
                let _ = app_progress.emit(
                    "model-progress",
                    ModelProgressEvent {
                        model_id: file_label.clone(),
                        file_index: index as u32 + 1,
                        file_count: total_files as u32,
                        downloaded: done,
                        total,
                    },
                );
            })?;
        }
        tracing::info!("auto-tags bundle install finished");
        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(format!("auto-tags install task failed: {e}")))??;

    if let Ok(mut prefs) = preferences::load(&app_data) {
        prefs.ai.object_detection = true;
        let _ = preferences::save(&app_data, &prefs);
    }

    let state = app.state::<AppState>();
    state.tags.invalidate();
    state.tags.kick();
    // Prefer a fresh connection so a busy worker write cannot hold up the UI
    // after download completes.
    let models_dir_for_status = models_dir.clone();
    let status = tauri::async_runtime::spawn_blocking(move || -> AppResult<ml::MlStatus> {
        let conn = open_db(&db_path)?;
        ml::status(&conn, &models_dir_for_status)
    })
    .await
    .map_err(|e| AppError::msg(format!("auto-tags status failed: {e}")))??;
    Ok(status)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagsStatusDto {
    pub model_ready: bool,
    pub enabled: bool,
    pub done: i64,
    pub total: i64,
}

#[tauri::command]
pub fn tags_status(state: State<'_, AppState>) -> AppResult<TagsStatusDto> {
    let prefs = preferences::load(&state.paths.app_data)?;
    state.with_db(|conn| {
        let cov = tags::coverage(conn)?;
        Ok(TagsStatusDto {
            model_ready: tags::tags_ready(conn)?,
            enabled: prefs.ai.object_detection,
            done: cov.done,
            total: cov.total,
        })
    })
}

#[tauri::command]
pub fn tags_progress(state: State<'_, AppState>) -> AppResult<tags::worker::TagsProgress> {
    state.tags.progress()
}

#[tauri::command]
pub fn kick_tags(state: State<'_, AppState>) -> AppResult<()> {
    state.tags.kick();
    Ok(())
}

#[tauri::command]
pub fn pause_tags(state: State<'_, AppState>) -> AppResult<()> {
    state.tags.pause();
    Ok(())
}

#[tauri::command]
pub fn clear_auto_tags(state: State<'_, AppState>) -> AppResult<usize> {
    let n = state.with_db(tags::clear_all)?;
    state.tags.invalidate();
    state.tags.kick();
    Ok(n)
}

#[tauri::command]
pub fn list_asset_labels(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<Vec<tags::AssetLabel>> {
    state.with_db(|conn| tags::list_for_asset(conn, &asset_id))
}

/// Download Florence-2 for on-device image captions.
#[tauri::command]
pub async fn install_captions_models(app: AppHandle) -> AppResult<ml::MlStatus> {
    let status = install_model_option(app.clone(), "florence-2-base-ft".into()).await?;
    let state = app.state::<AppState>();
    state.captions.invalidate();
    state.captions.kick();
    Ok(status)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionsStatusDto {
    pub model_ready: bool,
    pub enabled: bool,
    pub done: i64,
    pub total: i64,
}

#[tauri::command]
pub fn captions_status(state: State<'_, AppState>) -> AppResult<CaptionsStatusDto> {
    let prefs = preferences::load(&state.paths.app_data)?;
    state.with_db(|conn| {
        let cov = captions::coverage(conn)?;
        Ok(CaptionsStatusDto {
            model_ready: captions::captions_ready(conn)?,
            enabled: prefs.ai.captions,
            done: cov.done,
            total: cov.total,
        })
    })
}

#[tauri::command]
pub fn captions_progress(state: State<'_, AppState>) -> AppResult<captions::worker::CaptionsProgress> {
    state.captions.progress()
}

#[tauri::command]
pub fn kick_captions(state: State<'_, AppState>) -> AppResult<()> {
    state.captions.kick();
    Ok(())
}

#[tauri::command]
pub fn pause_captions(state: State<'_, AppState>) -> AppResult<()> {
    state.captions.pause();
    Ok(())
}

#[tauri::command]
pub fn clear_captions(state: State<'_, AppState>) -> AppResult<usize> {
    let n = state.with_db(captions::clear_all)?;
    state.captions.invalidate();
    state.captions.kick();
    Ok(n)
}

#[tauri::command]
pub fn get_asset_caption(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<Option<captions::AssetCaption>> {
    state.with_db(|conn| captions::get_for_asset(conn, &asset_id))
}

#[tauri::command]
pub fn list_import_runs(
    state: State<'_, AppState>,
    limit: u32,
) -> AppResult<Vec<history::ImportRun>> {
    state.with_db(|conn| history::list_import_runs(conn, limit.min(100)))
}

/// Full pluggable model library with install/active state per capability.
#[tauri::command]
pub fn model_library(state: State<'_, AppState>) -> AppResult<Vec<ml::LibraryOptionStatus>> {
    let prefs = preferences::load(&state.paths.app_data)?;
    state.with_db(|conn| ml::library_status(conn, &prefs.ai))
}

/// Download every file for a library option's bundle (user-initiated only).
#[tauri::command]
pub async fn install_model_option(app: AppHandle, option_id: String) -> AppResult<ml::MlStatus> {
    let opt = ml::library::option(&option_id)
        .ok_or_else(|| AppError::msg(format!("unknown model option '{option_id}'")))?;
    let bundle = opt.bundle.ok_or_else(|| {
        AppError::msg(format!(
            "'{}' is a {} backend and does not download",
            opt.name,
            match opt.runtime {
                ml::library::RuntimeKind::Native => "native",
                ml::library::RuntimeKind::Onnx => "onnx",
            }
        ))
    })?;

    let (db_path, models_dir, app_data) = {
        let state = app.state::<AppState>();
        (
            state.paths.db_path.clone(),
            state.paths.models_dir.clone(),
            state.paths.app_data.clone(),
        )
    };

    let app_for_job = app.clone();
    let dir_for_job = models_dir.clone();
    let bundle_name = bundle.to_string();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let conn = open_db(&db_path)?;
        let entries: Vec<_> = ml::catalog::bundle(&bundle_name).collect();
        let total_files = entries.len();
        for (index, entry) in entries.into_iter().enumerate() {
            let app_progress = app_for_job.clone();
            let file_label = entry.file_name.to_string();
            tracing::info!(
                model = entry.id,
                file = index + 1,
                of = total_files,
                option = option_id.as_str(),
                "installing model library file"
            );
            ml::download_and_install(&conn, &dir_for_job, entry, move |done, total| {
                let _ = app_progress.emit(
                    "model-progress",
                    ModelProgressEvent {
                        model_id: file_label.clone(),
                        file_index: index as u32 + 1,
                        file_count: total_files as u32,
                        downloaded: done,
                        total,
                    },
                );
            })?;
        }
        tracing::info!(
            option = option_id.as_str(),
            "model library install finished"
        );
        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(format!("model install task failed: {e}")))??;

    // Activate the option and enable the matching capability.
    if let Ok(mut prefs) = preferences::load(&app_data) {
        match opt.capability {
            ml::library::Capability::SemanticSearch => {
                prefs.ai.semantic_model = opt.id.to_string();
                prefs.ai.semantic_search = true;
            }
            ml::library::Capability::Ocr => {
                prefs.ai.ocr_model = opt.id.to_string();
                prefs.ai.ocr = true;
            }
            ml::library::Capability::Faces => {
                prefs.ai.faces_model = opt.id.to_string();
                prefs.ai.face_recognition = true;
            }
            ml::library::Capability::AutoTags => {
                prefs.ai.tags_model = opt.id.to_string();
                prefs.ai.object_detection = true;
            }
            ml::library::Capability::Captions => {
                prefs.ai.captions_model = opt.id.to_string();
                prefs.ai.captions = true;
            }
            ml::library::Capability::MemoryProse => {
                prefs.ai.prose_model = opt.id.to_string();
                prefs.ai.memory_prose = true;
            }
            _ => {}
        }
        let _ = preferences::save(&app_data, &prefs);
    }

    let state = app.state::<AppState>();
    state.embedder.invalidate();
    state.ocr.invalidate();
    state.faces.invalidate();
    state.tags.invalidate();
    state.captions.invalidate();
    memories::prose::invalidate_engine();
    state.embedder.kick();
    state.ocr.kick();
    state.faces.kick();
    state.tags.kick();
    state.captions.kick();
    state.with_db(|conn| ml::status(conn, &models_dir))
}

/// Switch the active backend for a capability. Optionally clear + re-run derived data.
#[tauri::command]
pub async fn set_active_model(
    app: AppHandle,
    option_id: String,
    reprocess: bool,
) -> AppResult<Vec<ml::LibraryOptionStatus>> {
    let opt = ml::library::option(&option_id)
        .ok_or_else(|| AppError::msg(format!("unknown model option '{option_id}'")))?;

    // Install first when the option needs files that aren't present.
    if let Some(bundle) = opt.bundle {
        let needs_install = {
            let state = app.state::<AppState>();
            state.with_db(|conn| {
                for entry in ml::catalog::bundle(bundle) {
                    if ml::installed_row(conn, entry.id)?.is_none() {
                        return Ok(true);
                    }
                }
                Ok(false)
            })?
        };
        if needs_install {
            install_model_option(app.clone(), option_id.clone()).await?;
        }
    }

    let app_data = {
        let state = app.state::<AppState>();
        state.paths.app_data.clone()
    };
    let mut prefs = preferences::load(&app_data)?;
    match opt.capability {
        ml::library::Capability::SemanticSearch => {
            prefs.ai.semantic_model = opt.id.to_string();
        }
        ml::library::Capability::Ocr => {
            prefs.ai.ocr_model = opt.id.to_string();
        }
        ml::library::Capability::Faces => {
            prefs.ai.faces_model = opt.id.to_string();
        }
        ml::library::Capability::AutoTags => {
            prefs.ai.tags_model = opt.id.to_string();
        }
        ml::library::Capability::Captions => {
            prefs.ai.captions_model = opt.id.to_string();
            prefs.ai.captions = true;
        }
        ml::library::Capability::MemoryProse => {
            prefs.ai.prose_model = opt.id.to_string();
            prefs.ai.memory_prose = true;
        }
        _ => {}
    }
    preferences::save(&app_data, &prefs)?;

    let state = app.state::<AppState>();
    state.embedder.invalidate();
    state.ocr.invalidate();
    state.faces.invalidate();
    state.tags.invalidate();
    state.captions.invalidate();
    memories::prose::invalidate_engine();

    if reprocess {
        let faces_dir = state.paths.faces_dir.clone();
        match opt.capability {
            ml::library::Capability::SemanticSearch => {
                let _ = state.with_db(ml::clear_embeddings);
                state.embedder.kick();
            }
            ml::library::Capability::Ocr => {
                let _ = state.with_db(ocr::clear_all);
                state.ocr.kick();
            }
            ml::library::Capability::Faces => {
                let _ = state.with_db(|conn| faces::clear_all(conn, &faces_dir));
                state.faces.kick();
            }
            ml::library::Capability::AutoTags => {
                let _ = state.with_db(tags::clear_all);
                state.tags.kick();
            }
            ml::library::Capability::Captions => {
                let _ = state.with_db(captions::clear_all);
                state.captions.kick();
            }
            _ => {}
        }
    } else {
        state.embedder.kick();
        state.ocr.kick();
        state.faces.kick();
        state.tags.kick();
        state.captions.kick();
    }

    state.with_db(|conn| ml::library_status(conn, &prefs.ai))
}

/// Wipe derived AI data for the chosen capabilities and re-queue background work.
///
/// `kinds` accepts any of: `"semantic"`, `"ocr"`, `"faces"`, `"tags"`, `"captions"`, `"all"`.
#[tauri::command]
pub fn reprocess_ai(state: State<'_, AppState>, kinds: Vec<String>) -> AppResult<ReprocessResult> {
    let mut want_semantic = false;
    let mut want_ocr = false;
    let mut want_faces = false;
    let mut want_tags = false;
    let mut want_captions = false;
    for kind in &kinds {
        if kind == "all" {
            want_semantic = true;
            want_ocr = true;
            want_faces = true;
            want_tags = true;
            want_captions = true;
            continue;
        }
        match ml::library::Capability::from_str(kind) {
            Some(ml::library::Capability::SemanticSearch) => want_semantic = true,
            Some(ml::library::Capability::Ocr) => want_ocr = true,
            Some(ml::library::Capability::Faces) => want_faces = true,
            Some(ml::library::Capability::AutoTags) => want_tags = true,
            Some(ml::library::Capability::Captions) => want_captions = true,
            Some(ml::library::Capability::MemoryProse) => {
                // Prose is on-demand; clearing cache is enough.
            }
            Some(ml::library::Capability::Duplicates | ml::library::Capability::BlurDetection) => {
                // Native capabilities — nothing to reprocess in AI queues.
            }
            None => {
                return Err(AppError::msg(format!(
                    "unknown reprocess kind: {kind} (use semantic, ocr, faces, tags, captions, or all)"
                )));
            }
        }
    }
    if !want_semantic && !want_ocr && !want_faces && !want_tags && !want_captions {
        return Err(AppError::msg(
            "select at least one AI capability to reprocess",
        ));
    }

    let faces_dir = state.paths.faces_dir.clone();
    let result = state.with_db(|conn| {
        let mut out = ReprocessResult {
            embeddings_cleared: 0,
            ocr_cleared: 0,
            faces_cleared: 0,
            tags_cleared: 0,
            captions_cleared: 0,
        };
        if want_semantic {
            out.embeddings_cleared = ml::clear_embeddings(conn)?;
        }
        if want_ocr {
            out.ocr_cleared = ocr::clear_all(conn)?;
        }
        if want_faces {
            out.faces_cleared = faces::clear_all(conn, &faces_dir)?;
        }
        if want_tags {
            out.tags_cleared = tags::clear_all(conn)?;
        }
        if want_captions {
            out.captions_cleared = captions::clear_all(conn)?;
        }
        Ok(out)
    })?;

    if want_semantic {
        semantic::ann::invalidate_and_remove(&state.paths.app_data);
        state.embedder.invalidate();
        state.embedder.kick();
    }
    if want_ocr {
        state.ocr.invalidate();
        state.ocr.kick();
    }
    if want_faces {
        state.faces.invalidate();
        state.faces.kick();
    }
    if want_tags {
        state.tags.invalidate();
        state.tags.kick();
    }
    if want_captions {
        state.captions.invalidate();
        state.captions.kick();
    }
    Ok(result)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReprocessResult {
    pub embeddings_cleared: usize,
    pub ocr_cleared: usize,
    pub faces_cleared: usize,
    pub tags_cleared: usize,
    pub captions_cleared: usize,
}

#[tauri::command]
pub fn list_people(state: State<'_, AppState>) -> AppResult<Vec<faces::Person>> {
    state.with_db(faces::list_people)
}

#[tauri::command]
pub fn list_ignored_people(state: State<'_, AppState>) -> AppResult<Vec<faces::Person>> {
    state.with_db(faces::list_ignored_people)
}

/// Hide a person from People and search. The cluster is kept, so the same face
/// stays hidden when it turns up in future imports.
#[tauri::command]
pub fn set_person_ignored(
    state: State<'_, AppState>,
    person_id: String,
    ignored: bool,
) -> AppResult<()> {
    state.with_db(|conn| faces::set_ignored(conn, &person_id, ignored))
}

#[tauri::command]
pub fn list_person_assets(
    state: State<'_, AppState>,
    person_id: String,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| faces::list_person_assets(conn, &person_id, limit, offset))
}

#[tauri::command]
pub fn rename_person(state: State<'_, AppState>, person_id: String, name: String) -> AppResult<()> {
    state.with_db(|conn| faces::cluster::rename(conn, &person_id, &name))
}

#[tauri::command]
pub fn merge_people(state: State<'_, AppState>, into_id: String, from_id: String) -> AppResult<()> {
    state.with_db(|conn| faces::cluster::merge(conn, &into_id, &from_id))
}

#[tauri::command]
pub fn detach_face(state: State<'_, AppState>, face_id: String) -> AppResult<String> {
    state.with_db(|conn| faces::cluster::detach(conn, &face_id))
}

#[tauri::command]
pub fn list_asset_faces(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<Vec<faces::FaceBox>> {
    state.with_db(|conn| faces::list_asset_faces(conn, &asset_id))
}

#[tauri::command]
pub fn recluster_faces(state: State<'_, AppState>) -> AppResult<usize> {
    let n = state.with_db(faces::cluster::recluster_unnamed)?;
    state.faces.kick();
    Ok(n)
}

/// Distinct geotagged places, most-populated first, each with a cover thumbnail.
#[tauri::command]
pub fn list_places(state: State<'_, AppState>) -> AppResult<Vec<places::PlaceGroup>> {
    state.with_db(places::list_places)
}

/// Photos taken at a given place label.
#[tauri::command]
pub fn list_place_assets(
    state: State<'_, AppState>,
    label: String,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| places::list_place_assets(conn, &label, limit, offset))
}

/// Curated Memories v1 cards (On this day / weekend trips / person + place).
#[tauri::command]
pub fn list_memories(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<memories::MemorySummary>> {
    let limit = limit.unwrap_or(30);
    state.with_db(|conn| memories::list_memories(conn, limit))
}

#[tauri::command]
pub fn get_memory(
    state: State<'_, AppState>,
    memory_id: String,
) -> AppResult<memories::MemoryDetail> {
    // Summary + cached prose only — never run ONNX here.
    state.with_db(|conn| memories::get_memory(conn, &memory_id))
}

/// Generate (or return cached) memory prose off the UI thread.
#[tauri::command]
pub async fn enrich_memory_prose(
    state: State<'_, AppState>,
    memory_id: String,
) -> AppResult<memories::MemorySummary> {
    let prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    let app_data = state.paths.app_data.clone();
    let db_path = state.paths.db_path.clone();
    let enabled = prefs.ai.memory_prose;

    let mut detail = state.with_db(|conn| memories::get_memory(conn, &memory_id))?;
    if detail.summary.prose.is_some() || !enabled {
        return Ok(detail.summary);
    }

    let title = detail.summary.title.clone();
    let subtitle = detail.summary.subtitle.clone();
    let quote = detail.summary.quote.clone();
    let mid = detail.summary.id.clone();

    let prose = tauri::async_runtime::spawn_blocking(move || {
        memories::prose::enrich_prose_unlocked(
            &db_path,
            &app_data,
            &mid,
            &title,
            &subtitle,
            quote.as_deref(),
            enabled,
        )
    })
    .await
    .map_err(|e| AppError::msg(format!("memory prose task failed: {e}")))?;

    match prose {
        Ok(p) => detail.summary.prose = p,
        Err(e) => tracing::warn!(error = %e, memory_id = %memory_id, "memory prose failed"),
    }
    detail.summary.insight = memories::compose_insight(&detail.summary);
    Ok(detail.summary)
}

#[tauri::command]
pub async fn list_memory_assets(
    state: State<'_, AppState>,
    memory_id: String,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    let db_path = state.paths.db_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = open_db(&db_path)?;
        memories::list_memory_assets(&conn, &memory_id, limit, offset)
    })
    .await
    .map_err(|e| AppError::msg(format!("list memory assets failed: {e}")))?
}

/// Persist a memory as a normal album (user-initiated only).
#[tauri::command]
pub fn save_memory_as_album(
    state: State<'_, AppState>,
    memory_id: String,
    name: Option<String>,
) -> AppResult<Album> {
    let (album, asset_ids) = state.with_db(|conn| {
        memories::save_memory_as_album(conn, &memory_id, name)
    })?;
    if !asset_ids.is_empty() {
        let count = asset_ids.len();
        push_history(
            &state,
            "album",
            format!(
                "Saved memory as album “{}” with {count} photo(s)",
                album.name
            ),
            Some(&album.id),
            HistoryAction::RemoveFromAlbum {
                album_id: album.id.clone(),
                asset_ids: asset_ids.clone(),
            },
            HistoryAction::AddToAlbum {
                album_id: album.id.clone(),
                asset_ids,
            },
        )?;
    }
    Ok(album)
}

/// Download LaMini-Flan-T5 for optional memory prose.
#[tauri::command]
pub async fn install_prose_models(app: AppHandle) -> AppResult<ml::MlStatus> {
    let status = install_model_option(app.clone(), "lamini-flan-t5-248m".into()).await?;
    memories::prose::invalidate_engine();
    Ok(status)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProseStatusDto {
    pub model_ready: bool,
    pub enabled: bool,
}

#[tauri::command]
pub fn prose_status(state: State<'_, AppState>) -> AppResult<ProseStatusDto> {
    let prefs = preferences::load(&state.paths.app_data)?;
    state.with_db(|conn| {
        Ok(ProseStatusDto {
            model_ready: memories::prose::prose_ready(conn)?,
            enabled: prefs.ai.memory_prose,
        })
    })
}

#[tauri::command]
pub fn clear_memory_prose(state: State<'_, AppState>) -> AppResult<usize> {
    memories::prose::invalidate_engine();
    state.with_db(memories::prose::clear_all)
}

/// Live progress of the background GPS / reverse-geocode pass.
#[tauri::command]
pub fn places_progress(state: State<'_, AppState>) -> AppResult<places::worker::PlacesProgress> {
    state.places.progress()
}

#[tauri::command]
pub fn kick_places(state: State<'_, AppState>) -> AppResult<()> {
    state.places.kick();
    Ok(())
}

/// Drop all Places data so it can be rebuilt from originals' GPS EXIF.
#[tauri::command]
pub fn clear_places(state: State<'_, AppState>) -> AppResult<usize> {
    let n = state.with_db(places::clear_all)?;
    state.places.kick();
    Ok(n)
}

/// Natural-language search over CLIP embeddings.
///
/// Returns an empty list (not an error) when the model is not installed, so the
/// UI can fall back to FTS without branching on a special error code.
#[tauri::command]
pub async fn semantic_search(
    app: AppHandle,
    query: String,
    limit: u32,
) -> AppResult<Vec<AssetSummary>> {
    let state = app.state::<AppState>();
    let prefs = preferences::load(&state.paths.app_data)?;
    if !prefs.ai.semantic_search {
        return Ok(Vec::new());
    }
    let limit = limit.clamp(1, 500) as usize;
    let embedder = Arc::clone(&state.embedder);
    let db_path = state.paths.db_path.clone();
    let app_data = state.paths.app_data.clone();

    tauri::async_runtime::spawn_blocking(move || -> AppResult<Vec<AssetSummary>> {
        let Some(engine) = embedder.engine()? else {
            return Ok(Vec::new());
        };
        let embedding = semantic::worker::embed_query(&engine, &query)?;
        let conn = open_db(&db_path)?;
        let hits = semantic::search_by_vector(&conn, &app_data, &embedding, limit)?;
        if hits.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = hits.into_iter().map(|(id, _)| id).collect();
        list_assets_preserving_order(&conn, &ids)
    })
    .await
    .map_err(|e| AppError::msg(format!("semantic search task failed: {e}")))?
}

/// Fetch assets by id, preserving the caller's order (search ranking).
fn list_assets_preserving_order(conn: &Connection, ids: &[String]) -> AppResult<Vec<AssetSummary>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_id = std::collections::HashMap::with_capacity(ids.len());
    {
        let placeholders = ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                    created_at, captured_at, indexed_at, favorite, rating, color_label,
                    thumbnail_path, camera, lens, deleted_at
             FROM assets
             WHERE deleted_at IS NULL AND id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), search::map_asset)?;
        for row in rows.flatten() {
            by_id.insert(row.id.clone(), row);
        }
    }
    Ok(ids.iter().filter_map(|id| by_id.remove(id)).collect())
}

#[tauri::command]
pub fn find_duplicates(state: State<'_, AppState>) -> AppResult<Vec<DuplicateGroup>> {
    state.with_db(duplicates::all_duplicates)
}

/// Soft-focus / out-of-focus images (Laplacian variance ≤ threshold).
#[tauri::command]
pub fn list_blurry_assets(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<BlurryAsset>> {
    state.with_db(|conn| blur::list_blurry(conn, limit.unwrap_or(200), offset.unwrap_or(0)))
}

/// Score images that still lack a blur_score (uses thumbnails when present).
#[tauri::command]
pub fn scan_blur_scores(state: State<'_, AppState>, limit: Option<u32>) -> AppResult<usize> {
    state.with_db(|conn| blur::backfill_missing(conn, limit.unwrap_or(500)))
}

#[tauri::command]
pub fn list_assets_by_ids(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<AssetSummary>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    state.with_db(|conn| {
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, path, hash, perceptual_hash, media_type, width, height, duration_ms,
                    created_at, captured_at, indexed_at, favorite, rating, color_label,
                    thumbnail_path, camera, lens, deleted_at
             FROM assets
             WHERE id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = ids
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt.query_map(param_refs.as_slice(), search::map_asset)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
}

#[tauri::command]
pub fn soft_delete_assets(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let count = state.with_db(|conn| trash::soft_delete(conn, &ids))?;
    push_history(
        &state,
        "trash",
        format!("Moved {count} item(s) to trash"),
        None,
        HistoryAction::Restore {
            asset_ids: ids.clone(),
        },
        HistoryAction::SoftDelete { asset_ids: ids },
    )?;
    Ok(count)
}

#[tauri::command]
pub fn restore_assets(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<usize> {
    state.with_db(|conn| trash::restore(conn, &ids))
}

#[tauri::command]
pub fn list_trash(
    state: State<'_, AppState>,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AssetSummary>> {
    state.with_db(|conn| trash::list_trash(conn, limit, offset))
}

#[tauri::command]
pub fn purge_trash(state: State<'_, AppState>) -> AppResult<usize> {
    let expired_ids: Vec<String> = state.with_db(|conn| {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(trash::DEFAULT_RETENTION_DAYS))
            .to_rfc3339();
        let mut stmt =
            conn.prepare("SELECT id FROM assets WHERE deleted_at IS NOT NULL AND deleted_at < ?1")?;
        let rows = stmt.query_map(params![cutoff], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })?;
    let count = state.with_db(|conn| trash::purge_expired(conn, trash::DEFAULT_RETENTION_DAYS))?;
    let dropped = state.history.lock().invalidate_assets(&expired_ids);
    if dropped > 0 {
        tracing::info!(dropped, "cleared undo/redo entries for purged trash");
    }
    if count > 0 {
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "trash",
                &format!(
                    "Auto-removed {count} item(s) from trash after {} days (files on disk kept)",
                    trash::DEFAULT_RETENTION_DAYS
                ),
                None,
            )?;
            Ok(())
        })?;
        tracing::info!(count, "purged expired trash entries");
    }
    Ok(count)
}

#[tauri::command]
pub fn empty_trash(state: State<'_, AppState>) -> AppResult<trash::PermanentDeleteResult> {
    let trashed_ids: Vec<String> = state.with_db(|conn| {
        let mut stmt = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })?;
    let result = state.with_db(trash::empty_trash)?;
    let dropped = state.history.lock().invalidate_assets(&trashed_ids);
    state.with_db(|conn| {
        history::record_activity(
            conn,
            "trash",
            &format!(
                "Emptied trash · permanently deleted {} item(s)",
                result.removed_from_library
            ),
            None,
        )?;
        Ok(())
    })?;
    if dropped > 0 {
        tracing::info!(dropped, "cleared undo/redo entries for emptied trash");
    }
    tracing::info!(
        removed = result.removed_from_library,
        files = result.files_deleted,
        "emptied trash"
    );
    Ok(result)
}

#[tauri::command]
pub fn permanently_delete_assets(
    state: State<'_, AppState>,
    ids: Vec<String>,
    delete_files: bool,
) -> AppResult<trash::PermanentDeleteResult> {
    if ids.is_empty() {
        return Err(AppError::msg("no assets selected"));
    }
    let result = state.with_db(|conn| trash::permanently_delete(conn, &ids, delete_files))?;
    let dropped = state.history.lock().invalidate_assets(&ids);
    state.with_db(|conn| {
        history::record_activity(
            conn,
            "trash",
            &format!(
                "Permanently deleted {} item(s){}",
                result.removed_from_library,
                if delete_files {
                    " (files removed from disk)"
                } else {
                    ""
                }
            ),
            None,
        )?;
        Ok(())
    })?;
    if dropped > 0 {
        tracing::info!(
            dropped,
            count = ids.len(),
            "cleared undo/redo entries for permanently deleted assets"
        );
    }
    tracing::info!(
        removed = result.removed_from_library,
        files = result.files_deleted,
        delete_files,
        "permanent delete from library"
    );
    Ok(result)
}

#[tauri::command]
pub fn export_assets_zip(
    state: State<'_, AppState>,
    ids: Vec<String>,
    dest: String,
) -> AppResult<ExportResult> {
    let dest_path = PathBuf::from(&dest);
    let prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    let options = export::ExportOptions {
        strip_metadata: prefs.privacy.strip_metadata_on_export
            || prefs.import_export.strip_metadata,
        jpeg_quality: prefs.import_export.jpeg_quality,
        preserve_folder_structure: prefs.import_export.preserve_folder_structure,
        max_edge: prefs.import_export.export_max_edge,
        naming: prefs.import_export.export_naming,
    };
    let result =
        state.with_db(|conn| export::export_assets_to_zip(conn, &ids, &dest_path, options))?;
    let note = if result.missing > 0 || !result.errors.is_empty() {
        Some(format!(
            "{} missing · {} warning(s)",
            result.missing,
            result.errors.len()
        ))
    } else {
        None
    };
    state.with_db(|conn| {
        history::record_export(
            conn,
            &result.path,
            ids.len() as u32,
            result.exported,
            result.missing,
            note.as_deref(),
        )?;
        history::record_activity(
            conn,
            "export",
            &format!("Exported {} file(s) to {}", result.exported, result.path),
            Some(&result.path),
        )?;
        Ok(())
    })?;
    tracing::info!(
        path = %result.path,
        exported = result.exported,
        missing = result.missing,
        "exported selection to zip"
    );
    Ok(result)
}

/// Apply rotate / crop / exposure, then save over the original or as a sibling copy.
/// Clears CLIP embeddings for the resulting asset and resumes background embedding.
/// Also clears non-destructive edit revisions for the source asset after bake.
#[tauri::command]
pub fn apply_image_edit(
    state: State<'_, AppState>,
    asset_id: String,
    ops: EditOps,
    mode: SaveMode,
) -> AppResult<EditResult> {
    let thumbs = state.paths.thumbs_dir.clone();
    let result = state.with_db(|conn| edit::apply_edit(conn, &thumbs, &asset_id, &ops, mode))?;
    let label = match result.mode {
        SaveMode::Replace => format!("Edited “{}”", edit_file_name(&result.asset.path)),
        SaveMode::Copy => format!("Saved edited copy “{}”", edit_file_name(&result.asset.path)),
    };
    let _ = state
        .with_db(|conn| history::record_activity(conn, "edit", &label, Some(&result.asset.id)));
    state.embedder.kick();
    state.ocr.kick();
    state.faces.kick();
    Ok(result)
}

/// Append non-destructive edit ops (original file untouched).
#[tauri::command]
pub fn save_edit_ops(
    state: State<'_, AppState>,
    asset_id: String,
    ops: EditOps,
) -> AppResult<SavedEditOps> {
    let saved = state.with_db(|conn| edit::save_edit_ops(conn, &asset_id, &ops))?;
    let _ = state.with_db(|conn| {
        history::record_activity(conn, "edit", "Saved non-destructive edits", Some(&asset_id))
    });
    Ok(saved)
}

/// Latest non-destructive edit ops, or null when none.
#[tauri::command]
pub fn get_edit_ops(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<Option<SavedEditOps>> {
    state.with_db(|conn| edit::get_edit_ops(conn, &asset_id))
}

/// Newest-first revision list for the editor history strip.
#[tauri::command]
pub fn list_edit_revisions(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<Vec<EditRevisionSummary>> {
    state.with_db(|conn| edit::list_edit_revisions(conn, &asset_id))
}

/// Re-apply an older revision as the new latest (append-only).
#[tauri::command]
pub fn revert_edit_revision(
    state: State<'_, AppState>,
    asset_id: String,
    revision_id: String,
) -> AppResult<SavedEditOps> {
    state.with_db(|conn| edit::revert_edit_revision(conn, &asset_id, &revision_id))
}

/// Clear all non-destructive revisions for an asset (reset).
#[tauri::command]
pub fn clear_edit_ops(state: State<'_, AppState>, asset_id: String) -> AppResult<()> {
    state.with_db(|conn| edit::clear_edit_ops(conn, &asset_id))
}

fn edit_file_name(path: &str) -> String {
    PathBuf::from(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> AppResult<HistorySnapshot> {
    let activity = state.with_db(|conn| {
        let mut stacks = state.history.lock();
        stacks.prune_invalid(conn);
        history::list_activity(conn, 100)
    })?;
    let stacks = state.history.lock();
    Ok(HistorySnapshot {
        can_undo: stacks.can_undo(),
        can_redo: stacks.can_redo(),
        undo_stack: stacks.list_undo(),
        redo_stack: stacks.list_redo(),
        activity,
    })
}

#[tauri::command]
pub fn list_exports(state: State<'_, AppState>, limit: u32) -> AppResult<Vec<ExportRecord>> {
    state.with_db(|conn| history::list_exports(conn, limit.clamp(1, 200)))
}

#[tauri::command]
pub fn undo_last(state: State<'_, AppState>) -> AppResult<bool> {
    let entry = state.with_db(|conn| {
        let mut stacks = state.history.lock();
        Ok(stacks.pop_valid_undo(conn))
    })?;
    match entry {
        Some(entry) => {
            state.with_db(|conn| {
                history::apply_action(conn, &entry.undo)?;
                history::record_activity(
                    conn,
                    "undo",
                    &format!("Undo: {}", entry.label),
                    Some(&entry.id),
                )?;
                Ok(())
            })?;
            state.history.lock().redo.push(entry);
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
pub fn redo_last(state: State<'_, AppState>) -> AppResult<bool> {
    let entry = state.with_db(|conn| {
        let mut stacks = state.history.lock();
        Ok(stacks.pop_valid_redo(conn))
    })?;
    match entry {
        Some(entry) => {
            state.with_db(|conn| {
                history::apply_action(conn, &entry.redo)?;
                history::record_activity(
                    conn,
                    "redo",
                    &format!("Redo: {}", entry.label),
                    Some(&entry.id),
                )?;
                Ok(())
            })?;
            state.history.lock().undo.push(entry);
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
pub fn list_watched_folders(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    state.with_db(watcher::list_watched)
}

#[tauri::command]
pub fn remove_watched_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<bool> {
    let root = PathBuf::from(&path);
    let removed = state.with_db(|conn| watcher::remove_watched(conn, &root))?;
    if removed {
        if let Some(ws) = app.try_state::<Arc<watcher::WatcherService>>() {
            ws.remove_root(&root);
        }
    }
    Ok(removed)
}

// ---------------------------------------------------------------------------
// Privacy vault (encrypted "locked folder")
// ---------------------------------------------------------------------------

fn vault_status_snapshot(state: &AppState) -> AppResult<VaultStatus> {
    let session = state.vault.lock().as_ref().map(|s| s.vault_id.clone());
    state.with_db(|conn| {
        let total = vault::total_locked_count(conn)?;
        let configured = vault::is_configured(conn)?;
        if let Some(ref vault_id) = session {
            let summaries = vault::list_vaults(conn, Some(vault_id))?;
            let active = summaries.iter().find(|v| v.id == *vault_id);
            Ok(VaultStatus {
                configured,
                unlocked: true,
                recovery_configured: active.map(|v| v.recovery_configured).unwrap_or(false),
                vault_id: Some(vault_id.clone()),
                vault_name: active.map(|v| v.name.clone()),
                vault_path: active.map(|v| v.path.clone()),
                locked_count: active.map(|v| v.locked_count).unwrap_or(0),
                total_locked_count: total,
            })
        } else {
            Ok(VaultStatus {
                configured,
                unlocked: false,
                recovery_configured: false,
                vault_id: None,
                vault_name: None,
                vault_path: None,
                locked_count: total,
                total_locked_count: total,
            })
        }
    })
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> AppResult<VaultStatus> {
    vault_status_snapshot(&state)
}

#[tauri::command]
pub fn list_vaults(state: State<'_, AppState>) -> AppResult<Vec<VaultSummary>> {
    let active = state.vault.lock().as_ref().map(|s| s.vault_id.clone());
    state.with_db(|conn| vault::list_vaults(conn, active.as_deref()))
}

#[tauri::command]
pub fn setup_vault(
    state: State<'_, AppState>,
    name: String,
    vault_path: String,
    password: String,
) -> AppResult<VaultSetupResult> {
    let outcome = state.with_db(|conn| vault::setup(conn, &name, &vault_path, &password))?;
    *state.vault.lock() = Some(VaultSession {
        vault_id: outcome.vault_id.clone(),
        master_key: outcome.master_key,
    });
    state.with_db(|conn| {
        history::record_activity(
            conn,
            "vault",
            &format!("Created locked vault “{name}”"),
            Some(&vault_path),
        )
    })?;
    tracing::info!(name = %name, "locked vault set up and unlocked");
    Ok(VaultSetupResult {
        status: vault_status_snapshot(&state)?,
        recovery_code: outcome.recovery_code,
    })
}

#[tauri::command]
pub fn unlock_vault(
    state: State<'_, AppState>,
    vault_id: String,
    password: String,
) -> AppResult<VaultStatus> {
    let key = state.with_db(|conn| vault::unlock(conn, &vault_id, &password))?;
    *state.vault.lock() = Some(VaultSession {
        vault_id,
        master_key: key,
    });
    vault_status_snapshot(&state)
}

/// Reset a forgotten password using the one-time recovery code, then unlock.
#[tauri::command]
pub fn recover_vault(
    state: State<'_, AppState>,
    vault_id: String,
    recovery_code: String,
    new_password: String,
) -> AppResult<VaultStatus> {
    let key =
        state.with_db(|conn| vault::recover(conn, &vault_id, &recovery_code, &new_password))?;
    *state.vault.lock() = Some(VaultSession {
        vault_id: vault_id.clone(),
        master_key: key,
    });
    state.with_db(|conn| {
        history::record_activity(
            conn,
            "vault",
            "Reset locked vault password with recovery code",
            Some(&vault_id),
        )
    })?;
    vault_status_snapshot(&state)
}

/// Add a recovery code to a vault created before this feature existed.
#[tauri::command]
pub fn enable_vault_recovery(state: State<'_, AppState>) -> AppResult<String> {
    let (vault_id, key) = state.vault_session()?;
    let code = state.with_db(|conn| vault::enable_recovery(conn, &vault_id, &key))?;
    state.with_db(|conn| {
        history::record_activity(
            conn,
            "vault",
            "Enabled locked vault recovery",
            Some(&vault_id),
        )
    })?;
    Ok(code)
}

#[tauri::command]
pub fn lock_vault(state: State<'_, AppState>) -> AppResult<VaultStatus> {
    // Dropping the session zeroizes the in-memory master key.
    *state.vault.lock() = None;
    vault_status_snapshot(&state)
}

#[tauri::command]
pub fn lock_assets_to_vault(
    state: State<'_, AppState>,
    ids: Vec<String>,
    vault_id: String,
) -> AppResult<LockResult> {
    if ids.is_empty() {
        return Err(AppError::msg("no items selected"));
    }
    let key = state.require_vault(&vault_id)?;
    let result = state.with_db(|conn| vault::lock_assets(conn, &vault_id, &key, &ids))?;
    if result.locked > 0 {
        state.history.lock().invalidate_assets(&ids);
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "vault",
                &format!("Moved {} item(s) to a locked vault", result.locked),
                Some(&vault_id),
            )
        })?;
    }
    Ok(result)
}

/// Move an entire library album into the vault, keeping it grouped.
#[tauri::command]
pub fn lock_album_to_vault(
    state: State<'_, AppState>,
    album_id: String,
    vault_id: String,
) -> AppResult<LockResult> {
    let key = state.require_vault(&vault_id)?;
    // Capture members first: they leave the library, so any undo entry that
    // still points at them has to go with them.
    let member_ids: Vec<String> = state.with_db(|conn| {
        let mut stmt = conn.prepare("SELECT asset_id FROM album_assets WHERE album_id = ?1")?;
        let rows = stmt.query_map(params![album_id], |r| r.get(0))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })?;

    let result = state.with_db(|conn| vault::lock_album(conn, &vault_id, &key, &album_id))?;
    if result.locked > 0 {
        let mut stacks = state.history.lock();
        stacks.invalidate_album(&album_id);
        stacks.invalidate_assets(&member_ids);
        drop(stacks);
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "vault",
                &format!(
                    "Moved an album ({} item(s)) to a locked vault",
                    result.locked
                ),
                Some(&vault_id),
            )
        })?;
    }
    Ok(result)
}

/// Move an entire folder from disk into the vault, keeping it grouped.
#[tauri::command]
pub fn lock_folder_to_vault(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    vault_id: String,
) -> AppResult<LockResult> {
    let key = state.require_vault(&vault_id)?;
    let root = PathBuf::from(&path);
    let result = state.with_db(|conn| vault::lock_folder(conn, &vault_id, &key, &root))?;

    if result.locked > 0 {
        // The folder's contents are gone, so stop watching it for changes.
        let unwatched = state.with_db(|conn| watcher::remove_watched(conn, &root))?;
        if unwatched {
            if let Some(ws) = app.try_state::<Arc<watcher::WatcherService>>() {
                ws.remove_root(&root);
            }
        }
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "vault",
                &format!(
                    "Moved a folder ({} item(s)) to a locked vault",
                    result.locked
                ),
                Some(&path),
            )
        })?;
    }
    Ok(result)
}

#[tauri::command]
pub fn list_locked_albums(state: State<'_, AppState>) -> AppResult<Vec<LockedAlbum>> {
    let (vault_id, key) = state.vault_session()?;
    state.with_db(|conn| vault::list_locked_albums(conn, &vault_id, &key))
}

/// List locked items. Omitting `albumId` returns the loose items that aren't
/// inside a locked group.
#[tauri::command]
pub fn list_locked_assets(
    state: State<'_, AppState>,
    album_id: Option<String>,
) -> AppResult<Vec<LockedAsset>> {
    // Require an unlocked session before revealing vault contents.
    let (vault_id, key) = state.vault_session()?;
    state.with_db(|conn| vault::list_locked(conn, &vault_id, &key, album_id.as_deref()))
}

#[tauri::command]
pub fn vault_thumb(state: State<'_, AppState>, id: String) -> AppResult<Option<String>> {
    let (vault_id, key) = state.vault_session()?;
    state.with_db(|conn| vault::decrypt_thumb(conn, &vault_id, &key, &id))
}

#[tauri::command]
pub fn vault_media(state: State<'_, AppState>, id: String) -> AppResult<String> {
    let (vault_id, key) = state.vault_session()?;
    state.with_db(|conn| vault::decrypt_media(conn, &vault_id, &key, &id))
}

#[tauri::command]
pub fn move_out_locked_assets(
    state: State<'_, AppState>,
    ids: Vec<String>,
    dest: String,
) -> AppResult<MoveOutResult> {
    if ids.is_empty() {
        return Err(AppError::msg("no items selected"));
    }
    let (vault_id, key) = state.vault_session()?;
    let result = state.with_db(|conn| vault::move_out(conn, &vault_id, &key, &ids, &dest))?;
    if result.restored > 0 {
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "vault",
                &format!("Moved {} item(s) out of a locked vault", result.restored),
                Some(&dest),
            )
        })?;
    }
    Ok(result)
}

/// Move an entire locked group out, recreating its folder structure at `dest`.
#[tauri::command]
pub fn move_out_locked_album(
    state: State<'_, AppState>,
    album_id: String,
    dest: String,
) -> AppResult<MoveOutResult> {
    let (vault_id, key) = state.vault_session()?;
    let result =
        state.with_db(|conn| vault::move_out_album(conn, &vault_id, &key, &album_id, &dest))?;
    if result.restored > 0 {
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "vault",
                &format!(
                    "Moved a locked folder ({} item(s)) out of the vault",
                    result.restored
                ),
                Some(&dest),
            )
        })?;
    }
    Ok(result)
}

#[tauri::command]
pub fn delete_locked_album(state: State<'_, AppState>, album_id: String) -> AppResult<usize> {
    let (vault_id, key) = state.vault_session()?;
    let removed =
        state.with_db(|conn| vault::delete_locked_album(conn, &vault_id, &key, &album_id))?;
    if removed > 0 {
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "vault",
                &format!("Permanently deleted a locked folder ({removed} item(s))"),
                None,
            )
        })?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn delete_locked_assets(state: State<'_, AppState>, ids: Vec<String>) -> AppResult<usize> {
    if ids.is_empty() {
        return Err(AppError::msg("no items selected"));
    }
    // Deleting blobs does not need the key, but require unlock to avoid
    // destructive actions while the vault is locked.
    let (vault_id, key) = state.vault_session()?;
    let removed = state.with_db(|conn| vault::delete_locked(conn, &vault_id, &key, &ids))?;
    if removed > 0 {
        state.with_db(|conn| {
            history::record_activity(
                conn,
                "vault",
                &format!("Permanently deleted {removed} item(s) from a locked vault"),
                None,
            )
        })?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn convert_file_src(path: String) -> AppResult<String> {
    // Frontend should use Tauri convertFileSrc; this helps debug.
    Ok(path)
}

/// Bootstrap helper used from lib setup.
pub fn bootstrap_indexer(db_path: PathBuf, thumbs: PathBuf) -> Arc<IndexerQueue> {
    IndexerQueue::new(db_path, thumbs)
}

// ─── Plugin commands ──────────────────────────────────────────────────────────

/// List all installed plugins with their metadata and enabled state.
#[tauri::command]
pub fn list_plugins(state: State<'_, AppState>) -> AppResult<Vec<plugins::PluginEntry>> {
    let prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    let entries = plugins::scan(&state.paths.plugins_dir, &prefs.plugins.enabled);
    Ok(entries)
}

/// Enable or disable a plugin without removing its files.
#[tauri::command]
pub fn set_plugin_enabled(
    state: State<'_, AppState>,
    plugin_id: String,
    enabled: bool,
) -> AppResult<()> {
    let mut prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    if enabled {
        prefs.plugins.enabled.insert(plugin_id, true);
    } else {
        prefs.plugins.enabled.remove(&plugin_id);
    }
    preferences::save(&state.paths.app_data, &prefs)
}

/// Validate and copy a plugin folder (or a parent folder containing plugins)
/// into the plugins directory.  Returns the list of installed plugin ids.
/// If the chosen directory has no manifest but contains valid plugin sub-folders,
/// all of them are installed as a batch.
#[tauri::command]
pub fn install_plugin_dir(
    state: State<'_, AppState>,
    source_dir: String,
) -> AppResult<Vec<String>> {
    let src = std::path::PathBuf::from(&source_dir);
    plugins::install_plugin_dir(&src, &state.paths.plugins_dir)
}

/// Return the user's installed-plugins directory (`{app_data}/plugins/`).
#[tauri::command]
pub fn get_plugins_dir(state: State<'_, AppState>) -> AppResult<String> {
    Ok(state.paths.plugins_dir.display().to_string())
}

/// Create a new plugin folder with manifest, main.js, and README.
#[tauri::command]
pub fn create_plugin(
    state: State<'_, AppState>,
    spec: plugins::CreatePluginSpec,
) -> AppResult<plugins::CreatePluginResult> {
    let result = plugins::create_plugin(&state.paths.plugins_dir, spec)?;
    let mut prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    prefs.plugins.enabled.insert(result.id.clone(), true);
    preferences::save(&state.paths.app_data, &prefs)?;
    Ok(result)
}

/// Analyze plugin JavaScript for structure issues and inferred permissions.
#[tauri::command]
pub fn analyze_plugin_source(main_js: String) -> AppResult<plugins::PluginAnalysis> {
    Ok(plugins::analyze_main_js(&main_js))
}

/// Read installed plugin source files for editing.
#[tauri::command]
pub fn read_plugin_sources(
    state: State<'_, AppState>,
    plugin_id: String,
) -> AppResult<plugins::PluginSources> {
    plugins::read_sources(&state.paths.plugins_dir, &plugin_id)
}

/// Read plugin sources from any folder (e.g. first-party examples before install).
#[tauri::command]
pub fn read_plugin_sources_from_dir(source_dir: String) -> AppResult<plugins::PluginSources> {
    let path = std::path::PathBuf::from(source_dir);
    plugins::read_sources_from_dir(&path, None)
}

/// Save edits to an installed plugin. Permissions are inferred from main.js.
#[tauri::command]
pub fn save_plugin_draft(
    state: State<'_, AppState>,
    draft: plugins::SavePluginDraft,
) -> AppResult<plugins::SavePluginResult> {
    plugins::save_draft(&state.paths.plugins_dir, draft)
}

/// Copy an installed or example plugin into a new personal fork.
#[tauri::command]
pub fn fork_plugin(
    state: State<'_, AppState>,
    spec: plugins::ForkPluginSpec,
) -> AppResult<plugins::SavePluginResult> {
    let result = plugins::fork_plugin(&state.paths.plugins_dir, spec)?;
    let mut prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    prefs.plugins.enabled.insert(result.id.clone(), true);
    preferences::save(&state.paths.app_data, &prefs)?;
    Ok(result)
}

/// Return the absolute path to the bundled first-party example plugins directory.
#[tauri::command]
pub fn get_plugin_examples_dir(state: State<'_, AppState>) -> AppResult<String> {
    state
        .plugin_examples_dir
        .as_ref()
        .map(|path| path.display().to_string())
        .ok_or_else(|| {
            AppError::msg(
                "Examples directory not found. Clone the repository and look in plugins/examples/.",
            )
        })
}

/// A plugin entry from the examples / discovery catalogue.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailablePlugin {
    pub manifest: plugins::PluginManifest,
    /// Absolute path to the source directory (inside examples/).
    pub source_dir: String,
    /// Whether this plugin is already installed in the user's plugins directory.
    pub installed: bool,
    /// Whether the installed copy is enabled.
    pub enabled: bool,
}

/// Scan the first-party examples directory and return a catalogue of available
/// plugins annotated with their installed / enabled state.
#[tauri::command]
pub fn list_available_plugins(state: State<'_, AppState>) -> AppResult<Vec<AvailablePlugin>> {
    let Some(examples_dir) = state.plugin_examples_dir.as_ref() else {
        return Ok(Vec::new());
    };
    let prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    let Ok(entries) = std::fs::read_dir(examples_dir) else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let dir = entry.path();
        let Ok(manifest) = plugins::PluginManifest::load(&dir) else {
            continue;
        };
        let installed = state.paths.plugins_dir.join(&manifest.id).exists();
        let enabled = prefs.plugins.enabled.get(&manifest.id).copied().unwrap_or(false);
        result.push(AvailablePlugin {
            source_dir: dir.display().to_string(),
            installed,
            enabled,
            manifest,
        });
    }
    result.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    Ok(result)
}

/// Delete the plugin folder and remove its enabled-state entry from preferences.
#[tauri::command]
pub fn remove_plugin(state: State<'_, AppState>, plugin_id: String) -> AppResult<()> {
    plugins::remove_plugin_dir(&plugin_id, &state.paths.plugins_dir)?;
    let mut prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    prefs.plugins.enabled.remove(&plugin_id);
    preferences::save(&state.paths.app_data, &prefs)
}

/// Run a plugin action against the selected asset ids (preview or apply).
#[tauri::command]
pub async fn run_plugin_action(
    app: AppHandle,
    state: State<'_, AppState>,
    plugin_id: String,
    action_id: String,
    asset_ids: Vec<String>,
    mode: Option<String>,
) -> AppResult<plugins::host::ActionResult> {
    let prefs = preferences::load(&state.paths.app_data).unwrap_or_default();
    let enabled = prefs.plugins.enabled.get(&plugin_id).copied().unwrap_or(false);
    if !enabled {
        return Err(AppError::msg(format!(
            "PLUGIN_NOT_FOUND: plugin '{plugin_id}' is not enabled"
        )));
    }
    let dir = plugins::plugin_dir(&plugin_id, &state.paths.plugins_dir);
    if !dir.exists() {
        return Err(AppError::msg(format!(
            "PLUGIN_NOT_FOUND: plugin '{plugin_id}' is not installed"
        )));
    }

    let manifest = plugins::PluginManifest::load(&dir)?;
    let run_mode = mode.as_deref().unwrap_or("apply").to_string();
    let db_path = state.paths.db_path.clone();
    let plugin_dir = dir.clone();
    let action_id_job = action_id.clone();
    let asset_ids_job = asset_ids.clone();
    let app_for_job = app.clone();

    let progress_cb: plugins::host::ProgressCallback = Arc::new(move |event| {
        let _ = app_for_job.emit("plugin-run-progress", event);
    });

    let (action_result, record) = tauri::async_runtime::spawn_blocking(move || {
        let conn = open_db(&db_path)?;
        plugins::host::run_action(
            &plugin_dir,
            &action_id_job,
            &asset_ids_job,
            &run_mode,
            &conn,
            Some(progress_cb),
        )
    })
    .await
    .map_err(|e| AppError::msg(format!("plugin task failed: {e}")))??;

    // Always append the run record regardless of outcome.
    if let Err(e) = plugins::append_record(&dir, &record) {
        tracing::warn!(plugin = %plugin_id, error = %e, "failed to write plugin history");
    }

    let _ = app.emit(
        "plugin-run-progress",
        plugins::host::PluginRunProgressEvent {
            run_id: record.run_id.clone(),
            plugin_id: plugin_id.clone(),
            plugin_name: manifest.name,
            action_id,
            phase: if action_result.ok {
                "done".to_string()
            } else {
                "error".to_string()
            },
            current: record.assets_affected,
            total: record.assets_requested,
            message: Some(action_result.message.clone()),
            logs: record.log_lines,
        },
    );

    Ok(action_result)
}

/// Return the last N run records for a plugin (default 20, max 100), newest first.
#[tauri::command]
pub fn get_plugin_history(
    state: State<'_, AppState>,
    plugin_id: String,
    limit: Option<usize>,
) -> AppResult<Vec<plugins::PluginRunRecord>> {
    let limit = limit.unwrap_or(20).min(100);
    let dir = plugins::plugin_dir(&plugin_id, &state.paths.plugins_dir);
    plugins::read_records(&dir, limit)
}

/// Delete the history file for a single plugin.
#[tauri::command]
pub fn clear_plugin_history(state: State<'_, AppState>, plugin_id: String) -> AppResult<()> {
    let dir = plugins::plugin_dir(&plugin_id, &state.paths.plugins_dir);
    plugins::clear_history(&dir)
}

/// Delete history files for all installed plugins.
#[tauri::command]
pub fn clear_all_plugin_history(state: State<'_, AppState>) -> AppResult<()> {
    plugins::clear_all_history(&state.paths.plugins_dir)
}
