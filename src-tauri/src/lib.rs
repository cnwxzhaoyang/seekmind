mod docmind;

use std::fs;

use tauri::Manager;
use tauri_plugin_dialog::init as dialog_init;
use tauri_plugin_opener::init as opener_init;

pub fn reset_local_storage() -> Result<(), String> {
    let sqlite_path = docmind::storage::db::sqlite_database_path();
    let tantivy_dir = docmind::storage::fulltext::fulltext_index_dir();

    if sqlite_path.exists() {
        fs::remove_file(&sqlite_path).map_err(|error| error.to_string())?;
    }

    if tantivy_dir.exists() {
        fs::remove_dir_all(&tantivy_dir).map_err(|error| error.to_string())?;
    }

    Ok(())
}

pub fn run() {
    let database = tauri::async_runtime::block_on(docmind::storage::Database::open_or_init())
        .expect("failed to initialize DocMind SQLite database");

    tauri::Builder::default()
        .manage(database)
        .plugin(dialog_init())
        .plugin(opener_init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            if std::env::var("DOCMIND_OPEN_DEVTOOLS").ok().as_deref() == Some("1") {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::docmind::commands::list_index_dirs,
            crate::docmind::commands::search_documents,
            crate::docmind::commands::get_search_debug_report,
            crate::docmind::commands::list_documents_in_dir,
            crate::docmind::commands::list_document_chunks,
            crate::docmind::commands::refresh_document,
            crate::docmind::commands::list_search_history,
            crate::docmind::commands::list_recent_documents,
            crate::docmind::commands::list_favorites,
            crate::docmind::commands::get_index_status,
            crate::docmind::commands::get_parser_runtime,
            crate::docmind::commands::get_index_settings,
            crate::docmind::commands::open_file,
            crate::docmind::commands::toggle_result_favorite,
            crate::docmind::commands::refresh_index,
            crate::docmind::commands::refresh_index_dir,
            crate::docmind::commands::add_index_dir,
            crate::docmind::commands::import_paths,
            crate::docmind::commands::remove_index_dir,
            crate::docmind::commands::set_index_dir_enabled,
            crate::docmind::commands::save_index_settings,
            crate::docmind::commands::retry_failed_file,
            crate::docmind::commands::clear_all_indexes,
            crate::docmind::commands::pause_indexing,
            crate::docmind::commands::resume_indexing,
            crate::docmind::semantic::commands::get_embedding_model_status,
            crate::docmind::semantic::commands::get_semantic_debug_report,
            crate::docmind::semantic::commands::rebuild_semantic_embeddings,
            crate::docmind::semantic::commands::list_embedding_models,
            crate::docmind::semantic::commands::set_default_embedding_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocMind application");
}
