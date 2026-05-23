#![allow(dead_code)]

use super::models::{IndexStatusView, SearchDebugView};
use super::search::{normalize_query, normalize_search_text};
use super::storage::{indexer, Database};
use super::parser::python_parser_config_json;
use super::storage::types::IndexSettings;
use std::path::Path;

#[tauri::command]
pub async fn list_index_dirs(state: tauri::State<'_, Database>) -> Result<Vec<super::models::IndexDirView>, String> {
    state.list_index_dirs().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn search_documents(
    query: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::SearchResultView>, String> {
    state
        .search_documents(&query, limit)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_search_debug_report(
    query: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<SearchDebugView, String> {
    let hits = state
        .search_documents(&query, limit)
        .await
        .map_err(|error| error.to_string())?;
    let (sqlite_documents, sqlite_chunks) = state
        .debug_counts()
        .await
        .map_err(|error| error.to_string())?;
    state
        .record_search_history(&query, hits.len())
        .await
        .map_err(|error| error.to_string())?;

    Ok(SearchDebugView {
        query: query.clone(),
        normalized_terms: normalize_query(&query),
        normalized_search_text: normalize_search_text(&query),
        sqlite_documents,
        sqlite_chunks,
        tantivy_documents: state.tantivy_document_count(),
        hit_count: hits.len(),
        hits,
    })
}

#[tauri::command]
pub async fn list_documents_in_dir(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::DocumentView>, String> {
    state
        .list_documents_in_dir(&path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_document_chunks(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::ChunkView>, String> {
    state
        .list_document_chunks(&path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_index_status(
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    state.get_index_status().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_parser_runtime() -> Result<super::models::ParserRuntimeView, String> {
    let config = python_parser_config_json();
    let enabled = config
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let available = config
        .get("available")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let active = if enabled && available {
        "python"
    } else {
        "rust"
    }
    .to_string();

    Ok(super::models::ParserRuntimeView {
        enabled,
        available,
        active,
        python_bin: config
            .get("bin")
            .and_then(|value| value.as_str())
            .unwrap_or("python3")
            .to_string(),
        script_path: config
            .get("script")
            .and_then(|value| value.as_str())
            .unwrap_or("parser/docmind_parser/__main__.py")
            .to_string(),
        timeout_ms: config
            .get("timeout_ms")
            .and_then(|value| value.as_u64())
            .unwrap_or(30_000),
    })
}

#[tauri::command]
pub async fn open_file(path: String, state: tauri::State<'_, Database>) -> Result<(), String> {
    state.open_file(&path).await
}

#[tauri::command]
pub async fn list_search_history(
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::SearchHistoryView>, String> {
    state
        .list_search_history(limit as i64)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_recent_documents(
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::RecentDocumentView>, String> {
    state
        .list_recent_documents(limit as i64)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_favorites(
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::FavoriteView>, String> {
    state
        .list_favorites(limit as i64)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn toggle_result_favorite(
    path: String,
    heading: String,
    paragraph: Option<u32>,
    page: Option<u32>,
    file_name: String,
    state: tauri::State<'_, Database>,
) -> Result<bool, String> {
    state
        .toggle_result_favorite(&path, &heading, paragraph, page, &file_name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_index(
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    eprintln!("[DocMind] refresh_index start");
    let result = indexer::rebuild_all(&state).await;
    match result {
        Ok(status) => {
            eprintln!(
                "[DocMind] refresh_index ok docs={} chunks={} failed={}",
                status.indexed_docs, status.indexed_chunks, status.failed_files
            );
            Ok(status)
        }
        Err(error) => {
            eprintln!("[DocMind] refresh_index error: {error}");
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub async fn refresh_index_dir(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    eprintln!("[DocMind] refresh_index_dir start path={path}");
    let result = indexer::rebuild_dir(&state, &path).await;
    match result {
        Ok(status) => {
            eprintln!(
                "[DocMind] refresh_index_dir ok path={path} docs={} chunks={} failed={}",
                status.indexed_docs, status.indexed_chunks, status.failed_files
            );
            Ok(status)
        }
        Err(error) => {
            eprintln!("[DocMind] refresh_index_dir error path={path} err={error}");
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub async fn add_index_dir(path: String, state: tauri::State<'_, Database>) -> Result<(), String> {
    let normalized = path.trim();
    if normalized.is_empty() {
        return Err("目录路径不能为空".to_string());
    }

    let path_ref = Path::new(normalized);
    if !path_ref.exists() || !path_ref.is_dir() {
        return Err(format!("不是有效的目录: {normalized}"));
    }

    state
        .add_index_dir(normalized)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_index_dir(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_index_dir(&path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_index_dir_enabled(
    path: String,
    enabled: bool,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .set_index_dir_enabled(&path, enabled)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn retry_failed_file(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    indexer::retry_failed_file(&state, &path).await
}

#[tauri::command]
pub async fn clear_all_indexes(
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    eprintln!("[DocMind] clear_all_indexes start");
    state
        .clear_all_index_data()
        .await
        .map_err(|error| error.to_string())?;
    let status = state.get_index_status().await.map_err(|error| error.to_string())?;
    eprintln!(
        "[DocMind] clear_all_indexes ok docs={} chunks={} failed={}",
        status.indexed_docs, status.indexed_chunks, status.failed_files
    );
    Ok(status)
}

#[tauri::command]
pub async fn pause_indexing(
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    state
        .request_pause_current_task()
        .await
        .map_err(|error| error.to_string())?;
    state.get_index_status().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resume_indexing(
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    state
        .clear_pause_request()
        .await
        .map_err(|error| error.to_string())?;
    indexer::resume(&state).await
}

#[tauri::command]
pub async fn get_index_settings(
    state: tauri::State<'_, Database>,
) -> Result<super::models::IndexSettingsView, String> {
    let settings = state
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    Ok(super::models::IndexSettingsView {
        exclude_dirs: settings.exclude_dirs,
        exclude_exts: settings.exclude_exts,
        max_file_size_mb: settings.max_file_size_mb,
    })
}

#[tauri::command]
pub async fn save_index_settings(
    settings: super::models::IndexSettingsView,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    let settings = IndexSettings {
        exclude_dirs: settings.exclude_dirs,
        exclude_exts: settings.exclude_exts,
        max_file_size_mb: settings.max_file_size_mb,
    };
    state
        .save_index_settings(&settings)
        .await
        .map_err(|error| error.to_string())
}
