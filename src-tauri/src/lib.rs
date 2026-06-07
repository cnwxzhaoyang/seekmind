/*
 * @author MorningSun
 * @CreatedDate 2026/06/06
 * @Description DocMind Tauri application entry point and public helper wrappers.
 */

mod docmind;

use std::fs;

#[cfg(debug_assertions)]
use tauri::Manager;
use tauri_plugin_dialog::init as dialog_init;
use tauri_plugin_opener::init as opener_init;

pub fn reset_local_storage() -> Result<(), String> {
    let sqlite_path = docmind::storage::db::sqlite_database_path();
    let tantivy_dir = docmind::storage::fulltext::fulltext_index_dir();
    let legacy_tantivy_dir = {
        let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        base.join("DocMind").join("tantivy")
    };

    if sqlite_path.exists() {
        fs::remove_file(&sqlite_path).map_err(|error| error.to_string())?;
    }

    if tantivy_dir.exists() {
        fs::remove_dir_all(&tantivy_dir).map_err(|error| error.to_string())?;
    }

    if legacy_tantivy_dir.exists() {
        fs::remove_dir_all(&legacy_tantivy_dir).map_err(|error| error.to_string())?;
    }

    Ok(())
}

pub fn run_vision_ocr_helper(args: &[String]) -> Result<(), String> {
    docmind::vision_ocr::run_cli(args)
}

pub fn run() {
    let database = tauri::async_runtime::block_on(docmind::storage::Database::open_or_init())
        .expect("failed to initialize DocMind SQLite database");
    let network_proxy_settings = tauri::async_runtime::block_on(
        database.get_network_proxy_settings(),
    )
    .unwrap_or_else(|_| docmind::storage::types::NetworkProxySettings {
        enabled: false,
        proxy_url: String::new(),
    });
    docmind::sidecar::apply_network_proxy_environment(Some(&network_proxy_settings));

    let repair_database = database.clone();

    tauri::Builder::default()
        .manage(database)
        .plugin(dialog_init())
        .plugin(opener_init())
        .setup(move |_app| {
            let database = repair_database.clone();
            let app_handle = _app.handle().clone();
            tauri::async_runtime::spawn(async move {
                docmind::commands::repair_fulltext_index_if_needed(app_handle, database).await;
            });

            #[cfg(debug_assertions)]
            if std::env::var("DOCMIND_OPEN_DEVTOOLS").ok().as_deref() == Some("1") {
                if let Some(window) = _app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::docmind::commands::list_index_dirs,
            crate::docmind::commands::search_documents,
            crate::docmind::commands::get_search_debug_report,
            crate::docmind::commands::request_search_debug_report,
            crate::docmind::commands::list_documents_in_dir,
            crate::docmind::commands::list_document_chunks,
            crate::docmind::commands::read_preview_image_data_url,
            crate::docmind::commands::refresh_document,
            crate::docmind::commands::list_search_history,
            crate::docmind::commands::remove_search_history,
            crate::docmind::commands::list_recent_documents,
            crate::docmind::commands::remove_recent_document,
            crate::docmind::commands::list_recent_views,
            crate::docmind::commands::record_recent_view,
            crate::docmind::commands::list_favorites,
            crate::docmind::commands::remove_favorite,
            crate::docmind::collections::commands::list_collections,
            crate::docmind::collections::commands::create_collection,
            crate::docmind::collections::commands::update_collection,
            crate::docmind::collections::commands::delete_collection,
            crate::docmind::collections::commands::list_collection_items,
            crate::docmind::collections::commands::add_collection_item,
            crate::docmind::collections::commands::update_collection_item_note,
            crate::docmind::collections::commands::remove_collection_item,
            crate::docmind::collections::commands::export_collection_markdown,
            crate::docmind::collections::commands::list_tags,
            crate::docmind::collections::commands::list_target_tags,
            crate::docmind::collections::commands::create_tag,
            crate::docmind::collections::commands::update_tag,
            crate::docmind::collections::commands::delete_tag,
            crate::docmind::collections::commands::add_tag_to_target,
            crate::docmind::collections::commands::remove_tag_from_target,
            crate::docmind::commands::get_index_status,
            crate::docmind::commands::get_parser_runtime,
            crate::docmind::commands::get_index_settings,
            crate::docmind::commands::open_file,
            crate::docmind::commands::quick_look_file,
            crate::docmind::commands::toggle_result_favorite,
            crate::docmind::commands::refresh_index,
            crate::docmind::commands::refresh_index_dir,
            crate::docmind::commands::refresh_pdf_ocr_tasks,
            crate::docmind::commands::add_index_dir,
            crate::docmind::commands::import_paths,
            crate::docmind::commands::remove_index_dir,
            crate::docmind::commands::set_index_dir_enabled,
            crate::docmind::commands::save_index_settings,
            crate::docmind::commands::retry_failed_file,
            crate::docmind::commands::refresh_pdf_ocr_document,
            crate::docmind::commands::delete_document,
            crate::docmind::commands::clear_all_indexes,
            crate::docmind::commands::pause_indexing,
            crate::docmind::commands::resume_indexing,
            crate::docmind::qa::commands::ask_question,
            crate::docmind::qa::commands::cancel_qa_question,
            crate::docmind::qa::commands::get_qa_settings,
            crate::docmind::qa::commands::save_qa_settings,
            crate::docmind::qa::commands::get_network_proxy_settings,
            crate::docmind::qa::commands::save_network_proxy_settings,
            crate::docmind::qa::commands::list_qa_model_profiles,
            crate::docmind::qa::commands::save_qa_model_profile,
            crate::docmind::qa::commands::remove_qa_model_profile,
            crate::docmind::qa::commands::set_default_qa_model_profile,
            crate::docmind::qa::commands::test_qa_connection,
            crate::docmind::qa::commands::list_qa_history,
            crate::docmind::qa::commands::remove_qa_history,
            crate::docmind::qa::commands::create_qa_session,
            crate::docmind::qa::commands::list_qa_sessions,
            crate::docmind::qa::commands::list_qa_messages,
            crate::docmind::qa::commands::remove_qa_session,
            crate::docmind::qa::commands::update_qa_session_title,
            crate::docmind::qa::commands::export_qa_session_markdown,
            crate::docmind::semantic::commands::get_embedding_model_status,
            crate::docmind::semantic::commands::get_semantic_debug_report,
            crate::docmind::semantic::commands::rebuild_semantic_embeddings,
            crate::docmind::semantic::commands::list_embedding_models,
            crate::docmind::semantic::commands::set_default_embedding_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running DocMind application");
}
