mod albums;
mod commands;
mod db;
mod diagnostics;
mod duplicates;
pub mod error;
mod edit;
mod export;
mod faces;
mod history;
mod indexer;
mod logging;
mod ml;
mod models;
mod ocr;
mod preferences;
mod saved_searches;
mod search;
mod semantic;
mod smart;
mod state;
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

use tauri::Manager;

use crate::commands::*;
use crate::state::{AppPaths, AppState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            let paths = AppPaths::from_app_data(app_data)?;
            logging::init_logging(&paths.logs_dir)?;
            tracing::info!(app_data = %paths.app_data.display(), "app data directory");

            let conn = db::open_and_migrate(&paths.db_path)?;
            match thumbnails::repair_missing_thumbnails(&conn, &paths.thumbs_dir) {
                Ok(n) if n > 0 => {
                    tracing::info!(repaired = n, "regenerated missing thumbnails")
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "thumbnail repair skipped"),
            }
            let indexer =
                commands::bootstrap_indexer(paths.db_path.clone(), paths.thumbs_dir.clone());
            let embedder = semantic::worker::EmbedWorker::new(
                paths.db_path.clone(),
                paths.models_dir.clone(),
            );
            let ocr = ocr::worker::OcrWorker::new(
                paths.db_path.clone(),
                paths.app_data.clone(),
            );
            let faces = faces::worker::FaceWorker::new(
                paths.db_path.clone(),
                paths.app_data.clone(),
            );
            let state = AppState::new(
                paths,
                conn,
                Arc::clone(&indexer),
                Arc::clone(&embedder),
                Arc::clone(&ocr),
                Arc::clone(&faces),
            );

            let watch_service = Arc::new(watcher::WatcherService::new());
            let roots = state
                .with_db(watcher::load_watched_paths)
                .unwrap_or_default();
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
            semantic_search,
            install_ocr_models,
            ocr_status,
            ocr_progress,
            kick_ocr,
            clear_ocr_text,
            get_asset_text,
            install_face_models,
            faces_status,
            faces_progress,
            kick_faces,
            clear_face_data,
            list_people,
            list_ignored_people,
            set_person_ignored,
            list_person_assets,
            rename_person,
            merge_people,
            detach_face,
            list_asset_faces,
            recluster_faces,
            find_duplicates,
            list_assets_by_ids,
            soft_delete_assets,
            restore_assets,
            list_trash,
            purge_trash,
            empty_trash,
            permanently_delete_assets,
            export_assets_zip,
            apply_image_edit,
            get_developer_info,
            get_preferences,
            set_preferences,
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
