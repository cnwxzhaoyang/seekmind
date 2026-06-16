/*
 * @author MorningSun
 * @CreatedDate 2026/06/06
 * @Description SeekMind Tauri application entry point and public helper wrappers.
 */

pub mod seekmind;

use dirs::{cache_dir, data_dir};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(debug_assertions)]
use tauri::Manager;
use tauri_plugin_dialog::init as dialog_init;
use tauri_plugin_fs::init as fs_init;
use tauri_plugin_opener::init as opener_init;

fn remove_dir_if_exists(path: &Path, label: &str) -> Result<(), String> {
    if !path.exists() {
        eprintln!("[SeekMind] reset skip missing {} path={}", label, path.display());
        return Ok(());
    }

    fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    eprintln!("[SeekMind] reset removed {} path={}", label, path.display());
    Ok(())
}

fn reset_target_dirs() -> Vec<(PathBuf, &'static str)> {
    let mut targets = Vec::new();

    if let Some(parent) = seekmind::storage::db::sqlite_database_path().parent() {
        targets.push((parent.to_path_buf(), "sqlite data root"));
    }

    if let Some(base) = data_dir() {
        targets.push((base.join("SeekMindDev"), "legacy dev data root"));
        targets.push((base.join("SeekMind"), "legacy release data root"));
    }

    if let Some(base) = cache_dir() {
        targets.push((base.join("com.zhaoyang.SeekMind.dev"), "legacy dev tantivy cache root"));
        targets.push((base.join("com.zhaoyang.seekmind"), "legacy release tantivy cache root"));
    }

    if let Some(parent) = seekmind::storage::fulltext::fulltext_index_dir().parent() {
        targets.push((parent.to_path_buf(), "active tantivy cache root"));
    }

    if let Ok(cache_dir) = std::env::var("SEEKMIND_FASTEMBED_CACHE_DIR") {
        let cache_dir = cache_dir.trim();
        if !cache_dir.is_empty() {
            let cache_path = PathBuf::from(cache_dir);
            if let Some(parent) = cache_path.parent() {
                targets.push((parent.to_path_buf(), "fastembed cache root"));
            } else {
                targets.push((cache_path, "fastembed cache root"));
            }
        }
    }

    targets
}

pub fn reset_local_storage() -> Result<(), String> {
    // 修复：首次启动/真正初始化必须同时清理 SQLite、Tantivy、fastembed 缓存和旧版本数据根，避免只删一半导致旧状态回流。
    eprintln!("[SeekMind] reset local storage start");
    let mut seen = std::collections::BTreeSet::new();

    for (target, label) in reset_target_dirs() {
        let key = target.display().to_string();
        if !seen.insert(key) {
            continue;
        }
        remove_dir_if_exists(&target, label)?;
    }

    eprintln!("[SeekMind] reset local storage completed");
    Ok(())
}

pub fn run_vision_ocr_helper(args: &[String]) -> Result<(), String> {
    seekmind::vision_ocr::run_cli(args)
}

pub fn run() {
    if std::env::var("SEEKMIND_FORCE_FIRST_LAUNCH")
        .ok()
        .as_deref()
        == Some("1")
    {
        eprintln!("[SeekMind] first launch reset requested");
        if let Err(error) = reset_local_storage() {
            eprintln!("[SeekMind] first launch reset failed: {error}");
        }
    }

    seekmind::sidecar::log_fastembed_cache_diagnostics();
    seekmind::sidecar::prepare_fastembed_cache_for_runtime();

    let database = tauri::async_runtime::block_on(seekmind::storage::Database::open_or_init())
        .expect("failed to initialize SeekMind SQLite database");
    let network_proxy_settings = tauri::async_runtime::block_on(
        database.get_network_proxy_settings(),
    )
    .unwrap_or_else(|_| seekmind::storage::types::NetworkProxySettings {
        enabled: false,
        proxy_url: String::new(),
    });
    seekmind::sidecar::apply_network_proxy_environment(Some(&network_proxy_settings));

    let repair_database = database.clone();

    tauri::Builder::default()
        .manage(database)
        .plugin(dialog_init())
        .plugin(fs_init())
        .plugin(opener_init())
        .setup(move |_app| {
            let database = repair_database.clone();
            let app_handle = _app.handle().clone();
            tauri::async_runtime::spawn(async move {
                seekmind::commands::repair_fulltext_index_if_needed(app_handle, database).await;
            });

            #[cfg(debug_assertions)]
            if std::env::var("SEEKMIND_OPEN_DEVTOOLS")
                .ok()
                .or_else(|| std::env::var("SeekMind_OPEN_DEVTOOLS").ok())
                .as_deref()
                == Some("1")
            {
                if let Some(window) = _app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::seekmind::commands::list_index_dirs,
            crate::seekmind::commands::get_app_runtime_info,
            crate::seekmind::commands::check_app_update,
            crate::seekmind::commands::search_documents,
            crate::seekmind::commands::get_search_debug_report,
            crate::seekmind::commands::request_search_debug_report,
            crate::seekmind::commands::list_documents_in_dir,
            crate::seekmind::commands::list_document_chunks,
            crate::seekmind::commands::read_preview_image_data_url,
            crate::seekmind::commands::refresh_document,
            crate::seekmind::commands::list_search_history,
            crate::seekmind::commands::remove_search_history,
            crate::seekmind::commands::list_recent_documents,
            crate::seekmind::commands::remove_recent_document,
            crate::seekmind::commands::list_recent_views,
            crate::seekmind::commands::record_recent_view,
            crate::seekmind::commands::list_favorites,
            crate::seekmind::commands::remove_favorite,
            crate::seekmind::collections::commands::list_collections,
            crate::seekmind::collections::commands::create_collection,
            crate::seekmind::collections::commands::update_collection,
            crate::seekmind::collections::commands::delete_collection,
            crate::seekmind::collections::commands::list_collection_items,
            crate::seekmind::collections::commands::add_collection_item,
            crate::seekmind::collections::commands::update_collection_item_note,
            crate::seekmind::collections::commands::remove_collection_item,
            crate::seekmind::collections::commands::export_collection_markdown,
            crate::seekmind::collections::commands::list_tags,
            crate::seekmind::collections::commands::list_target_tags,
            crate::seekmind::collections::commands::create_tag,
            crate::seekmind::collections::commands::update_tag,
            crate::seekmind::collections::commands::delete_tag,
            crate::seekmind::collections::commands::add_tag_to_target,
            crate::seekmind::collections::commands::remove_tag_from_target,
            crate::seekmind::commands::get_index_status,
            crate::seekmind::commands::get_parser_runtime,
            crate::seekmind::commands::get_index_settings,
            crate::seekmind::commands::open_file,
            crate::seekmind::commands::quick_look_file,
            crate::seekmind::commands::toggle_result_favorite,
            crate::seekmind::commands::refresh_index,
            crate::seekmind::commands::refresh_index_dir,
            crate::seekmind::commands::refresh_pdf_ocr_tasks,
            crate::seekmind::commands::add_index_dir,
            crate::seekmind::commands::import_paths,
            crate::seekmind::commands::remove_index_dir,
            crate::seekmind::commands::set_index_dir_enabled,
            crate::seekmind::commands::save_index_settings,
            crate::seekmind::commands::retry_failed_file,
            crate::seekmind::commands::refresh_pdf_ocr_document,
            crate::seekmind::commands::delete_document,
            crate::seekmind::commands::clear_all_indexes,
            crate::seekmind::commands::pause_indexing,
            crate::seekmind::commands::resume_indexing,
            crate::seekmind::commands::export_log_markdown,
            crate::seekmind::qa::commands::ask_question,
            crate::seekmind::qa::commands::cancel_qa_question,
            crate::seekmind::qa::commands::get_qa_settings,
            crate::seekmind::qa::commands::save_qa_settings,
            crate::seekmind::qa::commands::get_network_proxy_settings,
            crate::seekmind::qa::commands::save_network_proxy_settings,
            crate::seekmind::qa::commands::list_qa_model_profiles,
            crate::seekmind::qa::commands::save_qa_model_profile,
            crate::seekmind::qa::commands::remove_qa_model_profile,
            crate::seekmind::qa::commands::set_default_qa_model_profile,
            crate::seekmind::qa::commands::test_qa_connection,
            crate::seekmind::qa::commands::list_qa_history,
            crate::seekmind::qa::commands::remove_qa_history,
            crate::seekmind::qa::commands::create_qa_session,
            crate::seekmind::qa::commands::list_qa_sessions,
            crate::seekmind::qa::commands::list_qa_messages,
            crate::seekmind::qa::commands::remove_qa_session,
            crate::seekmind::qa::commands::update_qa_session_title,
            crate::seekmind::qa::commands::export_qa_session_markdown,
            crate::seekmind::semantic::commands::get_embedding_model_status,
            crate::seekmind::semantic::commands::get_semantic_debug_report,
            crate::seekmind::semantic::commands::rebuild_semantic_embeddings,
            crate::seekmind::semantic::commands::list_embedding_models,
            crate::seekmind::semantic::commands::set_default_embedding_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running SeekMind application");
}
