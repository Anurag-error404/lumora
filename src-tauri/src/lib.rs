mod albums;
mod blur;
mod captions;
mod commands;
mod db;
mod diagnostics;
mod duplicates;
mod edit;
pub mod error;
mod export;
mod faces;
/// ONNX Runtime glibc < 2.38 link shim (Linux gnu only).
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(dead_code)]
mod glibc_compat;
mod history;
mod indexer;
mod logging;
mod ml;
mod models;
mod ocr;
mod places;
mod preferences;
mod prefs_runtime;
mod saved_searches;
mod search;
mod semantic;
mod smart;
mod state;
mod tags;
mod thumbnails;
mod trash;
mod vault;
mod views;
mod watcher;

#[cfg(test)]
mod perf_smoke;

/// Opening a vault folder needs no database, so the `lumora-vault` CLI can
/// reuse this without pulling in the rest of the app.
pub use vault::portable;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::Manager;

use crate::commands::*;
use crate::state::{AppPaths, AppState};

fn enqueue_auto_scan(
    db_path: &std::path::Path,
    app_data: &std::path::Path,
    indexer: &Arc<indexer::queue::IndexerQueue>,
    force_launch: bool,
) {
    let prefs = preferences::load(app_data).unwrap_or_default();
    let interval_secs = match prefs.library.auto_scan.as_str() {
        "on_launch" if force_launch => Some(0),
        "hourly" => Some(60 * 60),
        "daily" => Some(24 * 60 * 60),
        _ => None,
    };
    let Some(interval_secs) = interval_secs else {
        return;
    };
    let stamp = app_data.join(".last_auto_scan");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let previous = std::fs::read_to_string(&stamp)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if interval_secs > 0 && now.saturating_sub(previous) < interval_secs {
        return;
    }
    let Ok(conn) = state::open_db(db_path) else {
        return;
    };
    let Ok(roots) = watcher::load_watched_paths(&conn) else {
        return;
    };
    for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();
            if path.is_file() && indexer::is_indexable_media(path, &prefs.library.ignore_patterns) {
                indexer.enqueue(indexer::queue::IndexJob::Upsert {
                    path: path.to_path_buf(),
                    generate_thumb: true,
                });
            }
        }
    }
    let _ = std::fs::write(stamp, now.to_string());
}

fn spawn_auto_scan(
    db_path: std::path::PathBuf,
    app_data: std::path::PathBuf,
    indexer: Arc<indexer::queue::IndexerQueue>,
) {
    enqueue_auto_scan(&db_path, &app_data, &indexer, true);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(60));
        enqueue_auto_scan(&db_path, &app_data, &indexer, false);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            let paths = AppPaths::from_app_data(app_data)?;
            preferences::set_app_data_dir(paths.app_data.clone());
            logging::init_logging(&paths.logs_dir)?;
            tracing::info!(app_data = %paths.app_data.display(), "app data directory");

            let conn = db::open_and_migrate(&paths.db_path)?;
            // Defer thumbnail / face-crop repair off the critical startup path so
            // the window can appear while background maintenance catches up.
            let repair_db = paths.db_path.clone();
            let repair_thumbs = paths.thumbs_dir.clone();
            std::thread::spawn(move || {
                let Ok(conn) = db::open_and_migrate(&repair_db) else {
                    return;
                };
                match thumbnails::repair_missing_thumbnails(&conn, &repair_thumbs) {
                    Ok(n) if n > 0 => {
                        tracing::info!(repaired = n, "regenerated missing thumbnails")
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "thumbnail repair skipped"),
                }
                match faces::repair_missing_face_crops(&conn) {
                    Ok(n) if n > 0 => {
                        tracing::info!(cleared = n, "cleared missing face crop paths")
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "face crop repair skipped"),
                }
            });
            let indexer =
                commands::bootstrap_indexer(paths.db_path.clone(), paths.thumbs_dir.clone());
            let prefs_boot = preferences::load(&paths.app_data).unwrap_or_default();
            let embedder =
                semantic::worker::EmbedWorker::new(paths.db_path.clone(), paths.app_data.clone());
            let ocr = ocr::worker::OcrWorker::new(paths.db_path.clone(), paths.app_data.clone());
            let faces =
                faces::worker::FaceWorker::new(paths.db_path.clone(), paths.app_data.clone());
            let places =
                places::worker::PlacesWorker::new(paths.db_path.clone(), paths.app_data.clone());
            let tags = tags::worker::TagsWorker::new(paths.db_path.clone(), paths.app_data.clone());
            let captions =
                captions::worker::CaptionsWorker::new(paths.db_path.clone(), paths.app_data.clone());
            let state = AppState::new(
                paths,
                conn,
                Arc::clone(&indexer),
                Arc::clone(&embedder),
                Arc::clone(&ocr),
                Arc::clone(&faces),
                Arc::clone(&places),
                Arc::clone(&tags),
                Arc::clone(&captions),
            );

            spawn_auto_scan(
                state.paths.db_path.clone(),
                state.paths.app_data.clone(),
                Arc::clone(&indexer),
            );

            let watch_service =
                Arc::new(watcher::WatcherService::new(state.paths.app_data.clone()));
            let watch_enabled = prefs_boot.library.watch_folders_enabled;
            watch_service.set_enabled(watch_enabled);
            let roots = if watch_enabled {
                state
                    .with_db(watcher::load_watched_paths)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            for root in &roots {
                watch_service.add_root(root.clone());
            }
            watch_service.start(Arc::clone(&indexer), roots);

            app.manage(state);
            app.manage(watch_service);

            tracing::info!("LUMORA ready");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_library_stats,
            import_folder,
            import_paths,
            cancel_import,
            list_assets,
            search_assets,
            get_index_progress,
            set_favorite,
            set_favorites,
            set_rating,
            set_ratings,
            set_color_label,
            set_color_labels,
            list_tags,
            list_tag_assets,
            get_library_facets,
            list_tag_browse_assets,
            create_tag,
            tag_asset,
            tag_assets,
            create_tag_and_assign,
            untag_asset,
            list_albums,
            list_saved_searches,
            record_recent_search,
            delete_saved_search,
            clear_recent_searches,
            get_asset_organisation,
            create_album,
            create_album_with_assets,
            rename_album,
            delete_album,
            add_to_album,
            add_assets_to_album,
            remove_from_album,
            list_album_assets,
            timeline_months,
            list_assets_for_month,
            list_recent,
            list_recently_viewed,
            record_asset_view,
            list_smart_collection,
            smart_collection_counts,
            ml_status,
            install_semantic_models,
            remove_ml_model,
            clear_ml_embeddings,
            semantic_status,
            embed_progress,
            kick_embedding,
            pause_embedding,
            semantic_search,
            install_ocr_models,
            ocr_status,
            ocr_progress,
            kick_ocr,
            pause_ocr,
            clear_ocr_text,
            get_asset_text,
            install_face_models,
            faces_status,
            faces_progress,
            kick_faces,
            pause_faces,
            clear_face_data,
            install_tags_models,
            install_captions_models,
            tags_status,
            tags_progress,
            kick_tags,
            pause_tags,
            clear_auto_tags,
            list_asset_labels,
            captions_status,
            captions_progress,
            kick_captions,
            pause_captions,
            clear_captions,
            get_asset_caption,
            list_import_runs,
            model_library,
            install_model_option,
            set_active_model,
            reprocess_ai,
            list_people,
            list_ignored_people,
            set_person_ignored,
            list_person_assets,
            rename_person,
            merge_people,
            detach_face,
            list_asset_faces,
            recluster_faces,
            list_places,
            list_place_assets,
            places_progress,
            kick_places,
            clear_places,
            find_duplicates,
            list_blurry_assets,
            scan_blur_scores,
            list_assets_by_ids,
            soft_delete_assets,
            restore_assets,
            list_trash,
            purge_trash,
            empty_trash,
            permanently_delete_assets,
            export_assets_zip,
            apply_image_edit,
            save_edit_ops,
            get_edit_ops,
            list_edit_revisions,
            revert_edit_revision,
            clear_edit_ops,
            get_developer_info,
            get_preferences,
            set_preferences,
            ping_user_activity,
            get_storage_summary,
            clear_thumbnail_cache,
            rebuild_thumbnail_cache,
            optimize_database,
            get_history,
            list_exports,
            undo_last,
            redo_last,
            list_watched_folders,
            remove_watched_folder,
            vault_status,
            list_vaults,
            setup_vault,
            unlock_vault,
            recover_vault,
            enable_vault_recovery,
            lock_vault,
            lock_assets_to_vault,
            lock_album_to_vault,
            lock_folder_to_vault,
            list_locked_albums,
            list_locked_assets,
            vault_thumb,
            vault_media,
            move_out_locked_assets,
            move_out_locked_album,
            delete_locked_assets,
            delete_locked_album,
            convert_file_src,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LUMORA");
}
