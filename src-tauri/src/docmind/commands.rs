#![allow(dead_code)]

use super::models::{
    ImportPathsView, ImportedPathView, IndexStatusView, SearchDebugReportEventView, SearchDebugView,
};
use super::parser::{office_converter_config_json, python_parser_config_json};
use super::search::{normalize_query, normalize_search_text};
use super::storage::types::IndexSettings;
use super::storage::{indexer, scanner, Database};
use crate::docmind::semantic::store as semantic_store;
use std::collections::HashSet;
use std::env;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::OnceLock;
use tauri::Emitter;

const VIRTUAL_IMPORT_DIR: &str = "virtual://临时导入";

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | b2 as u32;

        output.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(triple & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }

    output
}

fn image_mime_from_path(path: &Path, bytes: &[u8]) -> String {
    if let Some(kind) = infer::get(bytes) {
        if kind.mime_type().starts_with("image/") {
            return kind.mime_type().to_string();
        }
    }

    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn classify_failure(reason: &str, path: &str) -> (String, String) {
    let lower_reason = reason.to_lowercase();
    let lower_path = path.to_lowercase();

    if reason.contains("文件过大") {
        return ("文件过大".to_string(), "file_too_large".to_string());
    }
    if reason.contains("暂不支持 PDF")
        || reason.contains("不支持的文件类型")
        || lower_reason.contains("unsupported file type")
    {
        return ("不支持的格式".to_string(), "unsupported_format".to_string());
    }
    if lower_reason.contains("permission denied")
        || reason.contains("权限")
        || lower_reason.contains("access denied")
    {
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

fn normalize_import_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

fn parent_dir_path(path: &Path) -> String {
    path.parent()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string()
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
pub async fn list_index_dirs(
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::IndexDirView>, String> {
    state
        .list_index_dirs()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn search_documents(
    query: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::SearchResultView>, String> {
    if state
        .fulltext_repair_needed()
        .await
        .map_err(|error| error.to_string())?
    {
        return Err("全文索引正在修复，请稍后再搜索".to_string());
    }

    state
        .search_documents(&query, limit)
        .await
        .map_err(|error| error.to_string())
}

async fn build_search_debug_report(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<SearchDebugView, String> {
    let search_debug = database
        .search_documents_debug(query, limit)
        .await
        .map_err(|error| error.to_string())?;
    let (sqlite_documents, sqlite_chunks) = database
        .debug_counts()
        .await
        .map_err(|error| error.to_string())?;
    database
        .record_search_history(query, search_debug.hits.len())
        .await
        .map_err(|error| error.to_string())?;
    let query_rewrite_applied =
        !query.trim().is_empty() && !search_debug.rewritten_query.trim().is_empty();

    Ok(SearchDebugView {
        query: query.to_string(),
        normalized_terms: normalize_query(query),
        normalized_search_text: normalize_search_text(query),
        rewritten_query: search_debug.rewritten_query,
        rewritten_terms: search_debug.rewritten_terms,
        query_rewrite_applied,
        history_terms: search_debug.history_terms,
        history_rewrite_applied: search_debug.history_rewrite_applied,
        expanded_query: search_debug.expanded_query,
        sqlite_documents,
        sqlite_chunks,
        tantivy_documents: database.tantivy_document_count(),
        semantic_enabled: search_debug.semantic_enabled,
        semantic_weight: search_debug.semantic_weight,
        semantic_threshold: search_debug.semantic_threshold,
        keyword_hit_count: search_debug.keyword_hit_count,
        semantic_hit_count: search_debug.semantic_hit_count,
        semantic_candidate_count: search_debug.semantic_candidate_count,
        semantic_filtered_count: search_debug.semantic_filtered_count,
        semantic_fallback: search_debug.semantic_fallback,
        semantic_fallback_reason: search_debug.semantic_fallback_reason,
        search_mode: search_debug.search_mode,
        hit_count: search_debug.hits.len(),
        hits: search_debug.hits,
    })
}

#[tauri::command]
pub async fn get_search_debug_report(
    query: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<SearchDebugView, String> {
    build_search_debug_report(state.inner(), &query, limit).await
}

#[tauri::command]
pub async fn request_search_debug_report(
    app: tauri::AppHandle,
    request_id: String,
    query: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    let normalized_query = query.trim().to_string();
    if normalized_query.is_empty() {
        return Err("查询不能为空".to_string());
    }

    let database = state.inner().clone();
    let emit_app = app.clone();
    let started_at = chrono::Utc::now().to_rfc3339();
    let _ = app.emit(
        "docmind:search-debug-report",
        SearchDebugReportEventView {
            request_id: request_id.clone(),
            state: "running".to_string(),
            query: normalized_query.clone(),
            report: None,
            error: None,
            updated_at: started_at,
        },
    );

    tauri::async_runtime::spawn(async move {
        let result = build_search_debug_report(&database, &normalized_query, limit).await;
        match result {
            Ok(report) => {
                let _ = emit_app.emit(
                    "docmind:search-debug-report",
                    SearchDebugReportEventView {
                        request_id,
                        state: "completed".to_string(),
                        query: normalized_query,
                        report: Some(report),
                        error: None,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
            Err(error) => {
                let _ = emit_app.emit(
                    "docmind:search-debug-report",
                    SearchDebugReportEventView {
                        request_id,
                        state: "failed".to_string(),
                        query: normalized_query,
                        report: None,
                        error: Some(error),
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
        }
    });

    Ok(())
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
pub async fn read_preview_image_data_url(path: String) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("图片路径不能为空".to_string());
    }

    let path_ref = Path::new(trimmed);
    let bytes = std::fs::read(path_ref).map_err(|error| format!("读取图片失败: {error}"))?;
    let mime_type = image_mime_from_path(path_ref, &bytes);
    if !mime_type.starts_with("image/") {
        return Err(format!("不是可预览的图片类型: {mime_type}"));
    }

    Ok(format!("data:{mime_type};base64,{}", base64_encode(&bytes)))
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
    let semantic_emit_app = app.clone();
    let semantic_progress_emitter: Arc<
        dyn Fn(crate::docmind::models::SemanticRebuildProgressView) + Send + Sync,
    > = Arc::new(
        move |payload: crate::docmind::models::SemanticRebuildProgressView| {
            let _ = semantic_emit_app.emit("docmind:semantic:rebuild-progress", payload);
        },
    );
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
                    let (document, chunks, blocks) =
                        scanner::convert_python_document(&file, parsed);
                    Ok((
                        document,
                        chunks,
                        blocks,
                        super::storage::types::ParserSource::Python,
                    ))
                }
                Err(error) => {
                    let warning = match error {
                        super::parser::ParserClientError::ParserFailed(parser_error) => format!(
                            "Python 解析失败：{} ({})",
                            parser_error.message, parser_error.code
                        ),
                        other => format!("Python 解析失败：{other}"),
                    };
                    if file
                        .path
                        .extension()
                        .and_then(|value| value.to_str())
                        .map(|value| value.eq_ignore_ascii_case("pdf"))
                        .unwrap_or(false)
                    {
                        Err(warning)
                    } else {
                        parser_warning = Some(
                            warning.replace("Python 解析失败：", "Python 解析失败，已回退 Rust："),
                        );
                        let document = scanner::extract_document_at(&file.dir_path, &file.path);
                        match document {
                            Ok(document) => {
                                let chunks = scanner::chunk_document(&document);
                                Ok((
                                    document,
                                    chunks,
                                    Vec::new(),
                                    super::storage::types::ParserSource::Rust,
                                ))
                            }
                            Err(reason) => Err(reason),
                        }
                    }
                }
            }
        } else {
            match scanner::extract_document_at(&file.dir_path, &file.path) {
                Ok(document) => {
                    let chunks = scanner::chunk_document(&document);
                    Ok((
                        document,
                        chunks,
                        Vec::new(),
                        super::storage::types::ParserSource::Rust,
                    ))
                }
                Err(reason) => Err(reason),
            }
        };

        match parsed_result {
            Ok((document, chunks, blocks, source)) => {
                if let Err(error) = database.store_document(&document, &chunks, &blocks).await {
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
                match database.document_id_by_path(&document.path).await {
                    Ok(Some(document_id)) => {
                        if let Err(error) = semantic_store::upsert_document_embeddings(
                            &database,
                            &document_id,
                            &document,
                            &chunks,
                            &task_job_id,
                            "document",
                            Some(&semantic_progress_emitter),
                        )
                        .await
                        {
                            if error.contains("embedding 模型下载或加载超时")
                                || error.contains("embedding_unavailable")
                                || error.contains("timed out")
                            {
                                if let Ok(semantic_status) =
                                    semantic_store::get_embedding_model_status(&database).await
                                {
                                    let warning = format!("语义索引暂不可用：{error}");
                                    let _ = emit_app.emit(
                                        "docmind:semantic:rebuild-progress",
                                        crate::docmind::models::SemanticRebuildProgressView {
                                            job_id: task_job_id.clone(),
                                            state: "failed".to_string(),
                                            message: "单文档语义向量更新已跳过".to_string(),
                                            source: "document".to_string(),
                                            model: semantic_status.model,
                                            total_chunks: semantic_status.sqlite_chunks,
                                            processed_chunks: 0,
                                            embedded_chunks: 0,
                                            current_document: path_string.clone(),
                                            current_chunk: String::new(),
                                            percent: 0,
                                            last_error: warning,
                                            updated_at: chrono::Utc::now().to_rfc3339(),
                                        },
                                    );
                                }
                            } else {
                                eprintln!(
                                    "[DocMind] semantic upsert failed for {path_string}: {error}"
                                );
                                if let Ok(semantic_status) =
                                    semantic_store::get_embedding_model_status(&database).await
                                {
                                    let _ = emit_app.emit(
                                        "docmind:semantic:rebuild-progress",
                                        crate::docmind::models::SemanticRebuildProgressView {
                                            job_id: task_job_id.clone(),
                                            state: "failed".to_string(),
                                            message: "单文档语义向量更新失败".to_string(),
                                            source: "document".to_string(),
                                            model: semantic_status.model,
                                            total_chunks: semantic_status.sqlite_chunks,
                                            processed_chunks: 0,
                                            embedded_chunks: 0,
                                            current_document: path_string.clone(),
                                            current_chunk: String::new(),
                                            percent: 0,
                                            last_error: error,
                                            updated_at: chrono::Utc::now().to_rfc3339(),
                                        },
                                    );
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        eprintln!("[DocMind] semantic upsert skipped: missing document id for {path_string}");
                    }
                    Err(error) => {
                        eprintln!("[DocMind] semantic upsert skipped: {error}");
                    }
                }
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
    state
        .get_index_status()
        .await
        .map_err(|error| error.to_string())
}

pub async fn repair_fulltext_index_if_needed(app: tauri::AppHandle, database: Database) {
    let needed = match database.fulltext_repair_needed().await {
        Ok(needed) => needed,
        Err(error) => {
            eprintln!("[DocMind] fulltext repair check failed: {error}");
            return;
        }
    };
    if !needed || !database.try_begin_index_job() {
        return;
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let _ = database
        .set_current_task(
            "正在修复全文索引",
            "使用本地 SQLite 数据重建搜索索引",
            "running",
            "",
            "",
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            None,
            false,
        )
        .await;
    if let Ok(status) = database.get_index_status().await {
        let _ = app.emit(
            "docmind:index-refresh-progress",
            super::models::IndexRefreshProgressView {
                job_id: job_id.clone(),
                state: "running".to_string(),
                message: "正在修复全文索引".to_string(),
                scope: "fulltext-repair".to_string(),
                path: String::new(),
                status,
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }

    let result = database
        .repair_empty_fulltext_index(|processed, total, file_name| {
            let progress = if total == 0 {
                100
            } else {
                ((processed.saturating_mul(100)) / total).min(100) as u8
            };
            let database = database.clone();
            let app = app.clone();
            let job_id = job_id.clone();
            tauri::async_runtime::spawn(async move {
                let _ = database
                    .set_current_task(
                        "正在修复全文索引",
                        "使用本地 SQLite 数据重建搜索索引",
                        "running",
                        "",
                        &file_name,
                        progress,
                        processed,
                        total,
                        processed,
                        0,
                        0,
                        0,
                        0,
                        None,
                        false,
                    )
                    .await;
                if let Ok(status) = database.get_index_status().await {
                    let _ = app.emit(
                        "docmind:index-refresh-progress",
                        super::models::IndexRefreshProgressView {
                            job_id,
                            state: "running".to_string(),
                            message: format!("正在修复全文索引：{processed}/{total}"),
                            scope: "fulltext-repair".to_string(),
                            path: file_name,
                            status,
                            updated_at: chrono::Utc::now().to_rfc3339(),
                        },
                    );
                }
            });
        })
        .await;

    if let Err(error) = result {
        eprintln!("[DocMind] fulltext repair failed: {error}");
        let _ = database
            .set_current_task(
                "全文索引修复失败",
                "请在状态页刷新索引",
                "failed",
                "",
                "",
                100,
                0,
                0,
                0,
                1,
                0,
                0,
                0,
                Some(&error.to_string()),
                false,
            )
            .await;
        if let Ok(status) = database.get_index_status().await {
            let _ = app.emit(
                "docmind:index-refresh-progress",
                super::models::IndexRefreshProgressView {
                    job_id: job_id.clone(),
                    state: "failed".to_string(),
                    message: "全文索引修复失败".to_string(),
                    scope: "fulltext-repair".to_string(),
                    path: String::new(),
                    status,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                },
            );
        }
    } else {
        let _ = database.clear_current_task().await;
        if let Ok(status) = database.get_index_status().await {
            let _ = app.emit(
                "docmind:index-refresh-progress",
                super::models::IndexRefreshProgressView {
                    job_id: job_id.clone(),
                    state: "completed".to_string(),
                    message: "全文索引修复完成".to_string(),
                    scope: "fulltext-repair".to_string(),
                    path: String::new(),
                    status,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                },
            );
        }
    }

    database.end_index_job();
}

#[tauri::command]
pub async fn get_parser_runtime() -> Result<super::models::ParserRuntimeView, String> {
    let config = python_parser_config_json();
    let office_config = office_converter_config_json();
    let system_locale = detect_system_locale();
    let system_language = detect_system_language(&system_locale);
    let tesseract_languages = available_tesseract_languages();
    let chinese_ocr_available = has_chinese_tesseract_language(&tesseract_languages);
    let chinese_ocr_warning = if system_language == "zh" && !chinese_ocr_available {
        Some("当前系统语言为中文，但未检测到中文 OCR 语言包（chi_sim / chi_tra）。扫描件中文识别可能失效，请安装 Tesseract 中文语言包。".to_string())
    } else {
        None
    };
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
        system_locale,
        system_language,
        tesseract_languages,
        chinese_ocr_available,
        chinese_ocr_warning,
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
        office_enabled: office_config
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        office_available: office_config
            .get("available")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        office_bin: office_config
            .get("bin")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        office_message: office_config
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        office_platform: office_config
            .get("platform")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

fn detect_system_locale() -> String {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = env::var(key) {
            let normalized = normalize_locale_value(&value);
            if !normalized.is_empty() {
                return normalized;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
        {
            if output.status.success() {
                let raw = String::from_utf8_lossy(&output.stdout);
                let normalized = normalize_locale_value(&raw);
                if !normalized.is_empty() {
                    return normalized;
                }
            }
        }
    }

    String::new()
}

fn detect_system_language(locale: &str) -> String {
    let lowered = locale.to_lowercase();
    if lowered.starts_with("zh") || lowered.contains("chinese") {
        "zh".to_string()
    } else if lowered.is_empty() {
        "unknown".to_string()
    } else {
        lowered
    }
}

fn normalize_locale_value(value: &str) -> String {
    let cleaned = value.trim().trim_matches(['"', '\'']);
    if cleaned.is_empty() {
        return String::new();
    }

    if cleaned.contains("zh") || cleaned.contains("Chinese") {
        return cleaned.to_string();
    }

    cleaned
        .split(|ch: char| matches!(ch, ',' | '[' | ']' | ' ' | '\n' | '\t'))
        .find(|part| {
            let part = part.trim_matches(['"', '\'']);
            !part.is_empty()
        })
        .unwrap_or(cleaned)
        .trim_matches(['"', '\''])
        .to_string()
}

fn available_tesseract_languages() -> Vec<String> {
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            for candidate in tesseract_binary_candidates() {
                let output = Command::new(candidate).arg("--list-langs").output();
                let Ok(output) = output else {
                    continue;
                };
                if !output.status.success() {
                    continue;
                }

                let stdout = String::from_utf8_lossy(&output.stdout);
                let languages: Vec<String> = stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .filter(|line| {
                        !line
                            .to_lowercase()
                            .starts_with("list of available languages")
                    })
                    .map(ToOwned::to_owned)
                    .collect();

                if !languages.is_empty() {
                    return languages;
                }
            }

            Vec::new()
        })
        .clone()
}

fn tesseract_binary_candidates() -> [&'static str; 4] {
    [
        "/opt/homebrew/bin/tesseract",
        "/usr/local/bin/tesseract",
        "/opt/local/bin/tesseract",
        "tesseract",
    ]
}

fn has_chinese_tesseract_language(languages: &[String]) -> bool {
    languages.iter().any(|lang| {
        let lowered = lang.to_lowercase();
        lowered == "chi_sim" || lowered == "chi_tra" || lowered.starts_with("chi_")
    })
}

#[tauri::command]
pub async fn open_file(path: String, state: tauri::State<'_, Database>) -> Result<(), String> {
    state.open_file(&path).await
}

#[tauri::command]
pub async fn quick_look_file(
    app: tauri::AppHandle,
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state.quick_look_file(&app, &path).await
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
pub async fn remove_search_history(
    query: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_search_history(&query)
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
pub async fn remove_recent_document(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_recent_document(&path)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_recent_views(
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<super::models::RecentViewEntry>, String> {
    state
        .list_recent_views(limit as i64)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn record_recent_view(
    target_type: String,
    target_id: String,
    title: String,
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .record_recent_view(&target_type, &target_id, &title, &path)
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
pub async fn remove_favorite(
    target: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_favorite(&target)
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
    let index_progress_app = emit_app.clone();
    let task_job_id = job_id.clone();
    let task_start_status = start_status.clone();
    let index_progress_emitter: Arc<dyn Fn(super::models::IndexRefreshProgressView) + Send + Sync> =
        Arc::new(move |payload: super::models::IndexRefreshProgressView| {
            let _ = index_progress_app.emit("docmind:index-refresh-progress", payload);
        });
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
        let result = indexer::rebuild_all(&database, &task_job_id, index_progress_emitter).await;
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
                let status = database
                    .get_index_status()
                    .await
                    .unwrap_or(task_start_status.clone());
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

    Ok(super::models::IndexRefreshStartView {
        job_id,
        status: start_status,
    })
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
    let index_progress_app = emit_app.clone();
    let task_job_id = job_id.clone();
    let path_for_task = normalized_path.clone();
    let task_start_status = start_status.clone();
    let index_progress_emitter: Arc<dyn Fn(super::models::IndexRefreshProgressView) + Send + Sync> =
        Arc::new(move |payload: super::models::IndexRefreshProgressView| {
            let _ = index_progress_app.emit("docmind:index-refresh-progress", payload);
        });
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
        let result = indexer::rebuild_dir(
            &database,
            &path_for_task,
            &task_job_id,
            index_progress_emitter,
        )
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
                let status = database
                    .get_index_status()
                    .await
                    .unwrap_or(task_start_status.clone());
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

    Ok(super::models::IndexRefreshStartView {
        job_id,
        status: start_status,
    })
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
pub async fn import_paths(
    paths: Vec<String>,
    state: tauri::State<'_, Database>,
) -> Result<ImportPathsView, String> {
    let settings = state
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;

    let existing_dirs = state
        .list_index_dirs()
        .await
        .map_err(|error| error.to_string())?;
    let mut known_dirs = existing_dirs
        .into_iter()
        .map(|dir| dir.path)
        .collect::<HashSet<_>>();

    let mut added_dirs = Vec::new();
    let mut imported_files = Vec::new();
    let mut skipped = Vec::new();
    let mut unsupported = Vec::new();
    let mut virtual_dir_used = false;

    for raw_path in paths {
        let normalized = normalize_import_path(&raw_path);
        if normalized.is_empty() {
            continue;
        }

        let path = Path::new(&normalized);
        if path.is_dir() {
            if !path.exists() {
                skipped.push(normalized);
                continue;
            }
            state
                .add_index_dir(&normalized)
                .await
                .map_err(|error| error.to_string())?;
            known_dirs.insert(normalized.clone());
            added_dirs.push(normalized);
            continue;
        }

        if path.is_file() {
            if !scanner::is_supported_document_path(path, &settings) {
                unsupported.push(normalized);
                continue;
            }

            let parent_dir = parent_dir_path(path);
            let (target_dir, is_virtual) =
                if !parent_dir.is_empty() && known_dirs.contains(&parent_dir) {
                    (parent_dir, false)
                } else {
                    if !virtual_dir_used {
                        state
                            .add_index_dir(VIRTUAL_IMPORT_DIR)
                            .await
                            .map_err(|error| error.to_string())?;
                        known_dirs.insert(VIRTUAL_IMPORT_DIR.to_string());
                        virtual_dir_used = true;
                    }
                    (VIRTUAL_IMPORT_DIR.to_string(), true)
                };

            imported_files.push(ImportedPathView {
                path: normalized,
                dir_path: target_dir,
                is_virtual,
            });
            continue;
        }

        skipped.push(normalized);
    }

    Ok(ImportPathsView {
        added_dirs,
        imported_files,
        virtual_dir: if virtual_dir_used {
            VIRTUAL_IMPORT_DIR.to_string()
        } else {
            String::new()
        },
        skipped,
        unsupported,
    })
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
    let status = state
        .get_index_status()
        .await
        .map_err(|error| error.to_string())?;
    eprintln!(
        "[DocMind] clear_all_indexes ok docs={} chunks={} failed={}",
        status.indexed_docs, status.indexed_chunks, status.failed_files
    );
    Ok(status)
}

#[tauri::command]
pub async fn pause_indexing(state: tauri::State<'_, Database>) -> Result<IndexStatusView, String> {
    state
        .request_pause_current_task()
        .await
        .map_err(|error| error.to_string())?;
    state
        .get_index_status()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn resume_indexing(state: tauri::State<'_, Database>) -> Result<IndexStatusView, String> {
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
        title_weight: settings.title_weight,
        filename_weight: settings.filename_weight,
        preference_weight: settings.preference_weight,
        prefer_favorites_enabled: settings.prefer_favorites_enabled,
        prefer_recent_enabled: settings.prefer_recent_enabled,
        prefer_history_enabled: settings.prefer_history_enabled,
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
        title_weight: settings.title_weight,
        filename_weight: settings.filename_weight,
        preference_weight: settings.preference_weight,
        prefer_favorites_enabled: settings.prefer_favorites_enabled,
        prefer_recent_enabled: settings.prefer_recent_enabled,
        prefer_history_enabled: settings.prefer_history_enabled,
    };
    state
        .save_index_settings(&settings)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_document(
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    let path = path.trim();

    // First clean up tables that reference the document
    sqlx::query(
        r#"
        DELETE FROM document_blocks
        WHERE document_id IN (
            SELECT id
            FROM documents
            WHERE path = ?
        )
        "#,
    )
    .bind(path)
    .execute(state.pool())
    .await
    .map_err(|error| format!("清除文档块失败: {error}"))?;

    sqlx::query(
        r#"
        DELETE FROM chunks
        WHERE document_id IN (
            SELECT id
            FROM documents
            WHERE path = ?
        )
        "#,
    )
    .bind(path)
    .execute(state.pool())
    .await
    .map_err(|error| format!("清除切片失败: {error}"))?;

    sqlx::query(
        r#"
        DELETE FROM failed_files
        WHERE file = ?
        "#,
    )
    .bind(path)
    .execute(state.pool())
    .await
    .map_err(|error| format!("清除失败记录失败: {error}"))?;

    state
        .remove_recent_document(path)
        .await
        .map_err(|error| format!("清除最近文档失败: {error}"))?;

    // Clean up search index, chunk_embeddings, and document record
    state
        .clear_document_by_path(path)
        .await
        .map_err(|error| format!("清除文档数据失败: {error}"))?;

    Ok(())
}
