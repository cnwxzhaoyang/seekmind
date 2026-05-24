#![allow(dead_code)]

use super::models::{IndexStatusView, SearchDebugView};
use super::search::{normalize_query, normalize_search_text};
use super::storage::{indexer, scanner, Database};
use super::parser::python_parser_config_json;
use super::storage::types::IndexSettings;
use std::path::Path;
use tauri::Emitter;

fn classify_failure(reason: &str, path: &str) -> (String, String) {
    let lower_reason = reason.to_lowercase();
    let lower_path = path.to_lowercase();

    if reason.contains("文件过大") {
        return ("文件过大".to_string(), "file_too_large".to_string());
    }
    if reason.contains("暂不支持 PDF") || reason.contains("不支持的文件类型") || lower_reason.contains("unsupported file type") {
        return ("不支持的格式".to_string(), "unsupported_format".to_string());
    }
    if lower_reason.contains("permission denied") || reason.contains("权限") || lower_reason.contains("access denied") {
        return ("权限不足".to_string(), "permission_denied".to_string());
    }
    if lower_reason.contains("no such file")
        || reason.contains("不存在")
        || reason.contains("找不到")
        || lower_reason.contains("not found")
    {
        return ("文件缺失".to_string(), "file_missing".to_string());
    }
    if lower_reason.contains("is a directory")
        || reason.contains("目录")
        || lower_reason.contains("invalid path")
        || lower_reason.contains("not a file")
    {
        return ("路径无效".to_string(), "invalid_path".to_string());
    }
    if lower_reason.contains("timed out") || lower_reason.contains("timeout") {
        return ("处理超时".to_string(), "timeout".to_string());
    }
    if lower_reason.contains("sqlite")
        || lower_reason.contains("database")
        || lower_reason.contains("pool timed out")
    {
        return ("数据库错误".to_string(), "database_error".to_string());
    }
    if lower_reason.contains("tantivy") || lower_reason.contains("index") {
        return ("索引错误".to_string(), "index_error".to_string());
    }
    if lower_reason.contains("io error") || lower_reason.contains("input/output") {
        return ("IO 错误".to_string(), "io_error".to_string());
    }
    if lower_path.ends_with(".pdf") {
        return ("PDF 解析失败".to_string(), "parse_error".to_string());
    }

    ("解析失败".to_string(), "parse_error".to_string())
}

struct IndexJobGuard {
    database: Database,
}

impl IndexJobGuard {
    fn new(database: Database) -> Self {
        Self { database }
    }
}

impl Drop for IndexJobGuard {
    fn drop(&mut self) {
        self.database.end_index_job();
    }
}

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
    let search_debug = state
        .search_documents_debug(&query, limit)
        .await
        .map_err(|error| error.to_string())?;
    let (sqlite_documents, sqlite_chunks) = state
        .debug_counts()
        .await
        .map_err(|error| error.to_string())?;
    state
        .record_search_history(&query, search_debug.hits.len())
        .await
        .map_err(|error| error.to_string())?;
    let query_rewrite_applied = !query.trim().is_empty() && !search_debug.rewritten_query.trim().is_empty();

    Ok(SearchDebugView {
        query: query.clone(),
        normalized_terms: normalize_query(&query),
        normalized_search_text: normalize_search_text(&query),
        rewritten_query: search_debug.rewritten_query,
        rewritten_terms: search_debug.rewritten_terms,
        query_rewrite_applied,
        sqlite_documents,
        sqlite_chunks,
        tantivy_documents: state.tantivy_document_count(),
        semantic_enabled: search_debug.semantic_enabled,
        semantic_weight: search_debug.semantic_weight,
        semantic_threshold: search_debug.semantic_threshold,
        keyword_hit_count: search_debug.keyword_hit_count,
        semantic_hit_count: search_debug.semantic_hit_count,
        semantic_candidate_count: search_debug.semantic_candidate_count,
        semantic_filtered_count: search_debug.semantic_filtered_count,
        search_mode: search_debug.search_mode,
        hit_count: search_debug.hits.len(),
        hits: search_debug.hits,
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
pub async fn refresh_document(
    app: tauri::AppHandle,
    path: String,
    dir_path: String,
    state: tauri::State<'_, Database>,
) -> Result<super::models::DocumentRefreshStartView, String> {
    let normalized_path = path.trim();
    let normalized_dir = dir_path.trim();
    if normalized_path.is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    if normalized_dir.is_empty() {
        return Err("目录路径不能为空".to_string());
    }

    let path_ref = Path::new(normalized_path);
    if !path_ref.exists() || !path_ref.is_file() {
        return Err(format!("不是有效的文件: {normalized_path}"));
    }

    let settings = state
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    let file = scanner::snapshot_supported_file(normalized_dir, path_ref, &settings)
        .map_err(|error| error.to_string())?;

    if !state.try_begin_index_job() {
        return Err("已有索引任务正在执行".to_string());
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let start_status = state
        .get_index_status()
        .await
        .map_err(|error| error.to_string())?;
    let database = state.inner().clone();
    let emit_app = app.clone();
    let path_string = normalized_path.to_string();
    let dir_string = normalized_dir.to_string();
    let file_name = file
        .path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| normalized_path.to_string());
    let task_job_id = job_id.clone();
    let task_start_status = start_status.clone();
    let parser_hint = if super::parser::python_parser_enabled()
        && super::parser::PythonParserClient::from_env().is_configured()
    {
        "python".to_string()
    } else {
        "rust".to_string()
    };
    let use_python = parser_hint == "python";

    let initial_payload = super::models::DocumentRefreshProgressView {
        job_id: job_id.clone(),
        state: "running".to_string(),
        message: "正在重新切片文档".to_string(),
        path: path_string.clone(),
        file_name: file_name.clone(),
        parser_source: parser_hint.clone(),
        warning: None,
        status: start_status.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let _ = app.emit("docmind:document-refresh-progress", initial_payload);

    tauri::async_runtime::spawn(async move {
        let _guard = IndexJobGuard::new(database.clone());
        let mut parser_warning: Option<String> = None;
        let parsed_result = if use_python {
            let client = super::parser::PythonParserClient::from_env();
            match client.parse_document_stream(&file.path, |event| {
                let mut message = event.message.clone();
                if event.stage == "chunk" && !event.current.is_empty() {
                    message = format!("{message} · {}", event.current);
                }
                let _ = emit_app.emit(
                    "docmind:document-refresh-progress",
                    super::models::DocumentRefreshProgressView {
                        job_id: task_job_id.clone(),
                        state: "running".to_string(),
                        message,
                        path: path_string.clone(),
                        file_name: file_name.clone(),
                        parser_source: "python".to_string(),
                        warning: event.warning.clone(),
                        status: task_start_status.clone(),
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }) {
                Ok(parsed) => {
                    let (document, chunks) = scanner::convert_python_document(&file, parsed);
                    Ok((document, chunks, super::storage::types::ParserSource::Python))
                }
                Err(error) => {
                    parser_warning = Some(match error {
                        super::parser::ParserClientError::ParserFailed(parser_error) => format!(
                            "Python 解析失败，已回退 Rust：{} ({})",
                            parser_error.message, parser_error.code
                        ),
                        other => format!("Python 解析失败，已回退 Rust：{other}"),
                    });
                    let document = scanner::extract_document_at(&file.dir_path, &file.path);
                    match document {
                        Ok(document) => {
                            let chunks = scanner::chunk_document(&document);
                            Ok((document, chunks, super::storage::types::ParserSource::Rust))
                        }
                        Err(reason) => Err(reason),
                    }
                }
            }
        } else {
            match scanner::extract_document_at(&file.dir_path, &file.path) {
                Ok(document) => {
                    let chunks = scanner::chunk_document(&document);
                    Ok((document, chunks, super::storage::types::ParserSource::Rust))
                }
                Err(reason) => Err(reason),
            }
        };

        match parsed_result {
            Ok((document, chunks, source)) => {
                if let Err(error) = database.store_document(&document, &chunks).await {
                    let reason = error.to_string();
                    let (category, code) = classify_failure(&reason, &path_string);
                    let _ = database
                        .record_failed_file(&path_string, &reason, &category, &code)
                        .await;
                    let status = database
                        .get_index_status()
                        .await
                        .unwrap_or_else(|_| task_start_status.clone());
                    let _ = emit_app.emit(
                        "docmind:document-refresh-progress",
                        super::models::DocumentRefreshProgressView {
                            job_id: task_job_id.clone(),
                            state: "failed".to_string(),
                            message: format!("文档切片失败：{reason}"),
                            path: path_string.clone(),
                            file_name: file_name.clone(),
                            parser_source: match source {
                                super::storage::types::ParserSource::Python => "python".to_string(),
                                super::storage::types::ParserSource::Rust => "rust".to_string(),
                            },
                            warning: None,
                            status,
                            updated_at: chrono::Utc::now().to_rfc3339(),
                        },
                    );
                    return;
                }

                let _ = database.clear_failed_file(&path_string).await;
                let _ = database.refresh_index_dir_stats(&dir_string).await;
                let status = database
                    .get_index_status()
                    .await
                    .unwrap_or_else(|_| task_start_status.clone());
                let warning = parser_warning;
                let message = if warning.is_some() {
                    "文档切片完成，但已从 Python 回退到 Rust".to_string()
                } else {
                    "文档切片完成".to_string()
                };
                let _ = emit_app.emit(
                    "docmind:document-refresh-progress",
                    super::models::DocumentRefreshProgressView {
                        job_id: task_job_id.clone(),
                        state: "completed".to_string(),
                        message,
                        path: path_string.clone(),
                        file_name: file_name.clone(),
                        parser_source: match source {
                            super::storage::types::ParserSource::Python => "python".to_string(),
                            super::storage::types::ParserSource::Rust => "rust".to_string(),
                        },
                        warning,
                        status,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
            Err(reason) => {
                let (category, code) = classify_failure(&reason, &path_string);
                let _ = database
                    .record_failed_file(&path_string, &reason, &category, &code)
                    .await;
                let status = database
                    .get_index_status()
                    .await
                    .unwrap_or_else(|_| task_start_status.clone());
                let _ = emit_app.emit(
                    "docmind:document-refresh-progress",
                    super::models::DocumentRefreshProgressView {
                        job_id: task_job_id,
                        state: "failed".to_string(),
                        message: format!("文档切片失败：{reason}"),
                        path: path_string,
                        file_name,
                        parser_source: parser_hint,
                        warning: None,
                        status,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
        }
    });

    Ok(super::models::DocumentRefreshStartView {
        job_id,
        status: start_status,
    })
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
    app: tauri::AppHandle,
    state: tauri::State<'_, Database>,
) -> Result<super::models::IndexRefreshStartView, String> {
    eprintln!("[DocMind] refresh_index start");
    if !state.try_begin_index_job() {
        return Err("已有索引任务正在执行".to_string());
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let start_status = match state.get_index_status().await {
        Ok(status) => status,
        Err(error) => {
            state.end_index_job();
            return Err(error.to_string());
        }
    };
    let database = state.inner().clone();
    let emit_app = app.clone();
    let task_job_id = job_id.clone();
    let task_start_status = start_status.clone();
    let initial_payload = super::models::IndexRefreshProgressView {
        job_id: job_id.clone(),
        state: "running".to_string(),
        message: "正在重新索引本地文档".to_string(),
        scope: "all".to_string(),
        path: String::new(),
        status: start_status.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let _ = app.emit("docmind:index-refresh-progress", initial_payload);

    tauri::async_runtime::spawn(async move {
        let _guard = IndexJobGuard::new(database.clone());
        let progress_app = emit_app.clone();
        let result = indexer::rebuild_all(&database, &task_job_id, move |payload| {
            let _ = progress_app.emit("docmind:index-refresh-progress", payload);
        })
        .await;
        match result {
            Ok(status) => {
                eprintln!(
                    "[DocMind] refresh_index ok docs={} chunks={} failed={}",
                    status.indexed_docs, status.indexed_chunks, status.failed_files
                );
                let _ = emit_app.emit(
                    "docmind:index-refresh-progress",
                    super::models::IndexRefreshProgressView {
                        job_id: task_job_id,
                        state: "completed".to_string(),
                        message: "目录索引完成".to_string(),
                        scope: "all".to_string(),
                        path: String::new(),
                        status,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
            Err(error) => {
                eprintln!("[DocMind] refresh_index error: {error}");
                let status = database.get_index_status().await.unwrap_or(task_start_status.clone());
                let _ = emit_app.emit(
                    "docmind:index-refresh-progress",
                    super::models::IndexRefreshProgressView {
                        job_id: task_job_id,
                        state: "failed".to_string(),
                        message: format!("目录索引失败：{error}"),
                        scope: "all".to_string(),
                        path: String::new(),
                        status,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
        }
    });

    Ok(super::models::IndexRefreshStartView { job_id, status: start_status })
}

#[tauri::command]
pub async fn refresh_index_dir(
    app: tauri::AppHandle,
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<super::models::IndexRefreshStartView, String> {
    eprintln!("[DocMind] refresh_index_dir start path={path}");
    if !state.try_begin_index_job() {
        return Err("已有索引任务正在执行".to_string());
    }

    let normalized_path = path.trim().to_string();
    if normalized_path.is_empty() {
        state.end_index_job();
        return Err("目录路径不能为空".to_string());
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let start_status = match state.get_index_status().await {
        Ok(status) => status,
        Err(error) => {
            state.end_index_job();
            return Err(error.to_string());
        }
    };
    let database = state.inner().clone();
    let emit_app = app.clone();
    let task_job_id = job_id.clone();
    let path_for_task = normalized_path.clone();
    let task_start_status = start_status.clone();
    let initial_payload = super::models::IndexRefreshProgressView {
        job_id: job_id.clone(),
        state: "running".to_string(),
        message: "正在重新索引目录".to_string(),
        scope: "dir".to_string(),
        path: normalized_path.clone(),
        status: start_status.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let _ = app.emit("docmind:index-refresh-progress", initial_payload);

    tauri::async_runtime::spawn(async move {
        let _guard = IndexJobGuard::new(database.clone());
        let progress_app = emit_app.clone();
        let result = indexer::rebuild_dir(&database, &path_for_task, &task_job_id, move |payload| {
            let _ = progress_app.emit("docmind:index-refresh-progress", payload);
        })
        .await;
        match result {
            Ok(status) => {
                eprintln!(
                    "[DocMind] refresh_index_dir ok path={path_for_task} docs={} chunks={} failed={}",
                    status.indexed_docs, status.indexed_chunks, status.failed_files
                );
                let _ = emit_app.emit(
                    "docmind:index-refresh-progress",
                    super::models::IndexRefreshProgressView {
                        job_id: task_job_id,
                        state: "completed".to_string(),
                        message: "目录索引完成".to_string(),
                        scope: "dir".to_string(),
                        path: path_for_task,
                        status,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
            Err(error) => {
                eprintln!("[DocMind] refresh_index_dir error path={path_for_task} err={error}");
                let status = database.get_index_status().await.unwrap_or(task_start_status.clone());
                let _ = emit_app.emit(
                    "docmind:index-refresh-progress",
                    super::models::IndexRefreshProgressView {
                        job_id: task_job_id,
                        state: "failed".to_string(),
                        message: format!("目录索引失败：{error}"),
                        scope: "dir".to_string(),
                        path: path_for_task,
                        status,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
        }
    });

    Ok(super::models::IndexRefreshStartView { job_id, status: start_status })
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
        semantic_search_enabled: settings.semantic_search_enabled,
        semantic_weight: settings.semantic_weight,
        semantic_threshold: settings.semantic_threshold,
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
        semantic_search_enabled: settings.semantic_search_enabled,
        semantic_weight: settings.semantic_weight,
        semantic_threshold: settings.semantic_threshold,
    };
    state
        .save_index_settings(&settings)
        .await
        .map_err(|error| error.to_string())
}
