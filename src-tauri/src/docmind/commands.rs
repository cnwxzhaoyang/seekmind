#![allow(dead_code)]

use super::models::IndexStatusView;
use super::storage::{indexer, Database};
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
pub async fn get_index_status(
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    state.get_index_status().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn open_file(path: String, state: tauri::State<'_, Database>) -> Result<(), String> {
    state.open_file(&path).await
}

#[tauri::command]
pub async fn refresh_index(
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    indexer::rebuild_all(&state).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn refresh_index_dir(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<IndexStatusView, String> {
    indexer::rebuild_dir(&state, &path)
        .await
        .map_err(|error| error.to_string())
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
