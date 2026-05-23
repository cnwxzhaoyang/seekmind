#![allow(dead_code)]

use crate::docmind::models::IndexStatusView;
use crate::docmind::storage::scanner;
use crate::docmind::storage::Database;
use crate::docmind::storage::types::DocumentState;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

pub async fn rebuild_all(database: &Database) -> Result<IndexStatusView, sqlx::Error> {
    database.clear_pause_request().await?;
    database.clear_index_checkpoint().await?;
    let dir_paths = database.enabled_index_dir_paths().await?;
    database.clear_failed_files().await?;
    database.clear_current_task().await?;
    rebuild_paths(database, &dir_paths).await
}

pub async fn rebuild_dir(database: &Database, dir_path: &str) -> Result<IndexStatusView, sqlx::Error> {
    database.clear_pause_request().await?;
    database.clear_index_checkpoint().await?;
    let dir_paths = vec![dir_path.to_string()];
    rebuild_paths(database, &dir_paths).await
}

pub async fn resume(database: &Database) -> Result<IndexStatusView, String> {
    let checkpoint = database
        .load_index_checkpoint()
        .await
        .map_err(|error| error.to_string())?;

    let Some(checkpoint) = checkpoint else {
        return database
            .get_index_status()
            .await
            .map_err(|error| error.to_string());
    };

    let dir_paths: Vec<String> =
        serde_json::from_str(&checkpoint.dir_paths).map_err(|error| error.to_string())?;
    let pending_delete_paths: VecDeque<String> =
        serde_json::from_str(&checkpoint.pending_delete_paths).map_err(|error| error.to_string())?;
    let pending_update_paths: VecDeque<String> =
        serde_json::from_str(&checkpoint.pending_update_paths).map_err(|error| error.to_string())?;

    let settings = database
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    let mut existing_by_path = HashMap::<String, DocumentState>::new();
    for dir_path in &dir_paths {
        for state in database
            .document_states_in_dir(dir_path)
            .await
            .map_err(|error| error.to_string())?
        {
            existing_by_path.insert(state.path.clone(), state);
        }
    }

    let plan = IndexPlan {
        dir_paths,
        pending_delete_paths,
        pending_update_paths,
        total: checkpoint.total.max(0) as usize,
        processed: checkpoint.processed.max(0) as usize,
        succeeded: checkpoint.succeeded.max(0) as usize,
        failed: checkpoint.failed.max(0) as usize,
        updated: checkpoint.updated.max(0) as usize,
        skipped: checkpoint.skipped.max(0) as usize,
        deleted: checkpoint.deleted.max(0) as usize,
    };

    process_index_plan(database, plan, &settings, &existing_by_path)
        .await
        .map_err(|error| error.to_string())
}

pub async fn retry_failed_file(database: &Database, path: &str) -> Result<IndexStatusView, String> {
    let normalized = path.trim();
    if normalized.is_empty() {
        return Err("文件路径不能为空".to_string());
    }

    let path_buf = std::path::Path::new(normalized);
    if !path_buf.exists() || !path_buf.is_file() {
        return Err(format!("不是有效的文件: {normalized}"));
    }

    let dir_paths = database
        .enabled_index_dir_paths()
        .await
        .map_err(|error| error.to_string())?;
    let dir_path = dir_paths
        .iter()
        .filter(|candidate| normalized.starts_with(candidate.as_str()))
        .max_by_key(|candidate| candidate.len())
        .cloned()
        .ok_or_else(|| "找不到该文件所属的索引目录".to_string())?;

    let settings = database
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    let file = scanner::snapshot_supported_file(&dir_path, path_buf, &settings)?;
    match scanner::parse_document(&file) {
        Ok((document, chunks)) => {
            database
                .clear_document_by_path(normalized)
                .await
                .map_err(|error| error.to_string())?;
            database
                .store_document(&document, &chunks)
                .await
                .map_err(|error| error.to_string())?;
            database
                .clear_failed_file(normalized)
                .await
                .map_err(|error| error.to_string())?;
        }
        Err(reason) => {
            let (category, code) = classify_failure(&reason, normalized);
            database
                .record_failed_file(normalized, &reason, &category, &code)
                .await
                .map_err(|error| error.to_string())?;
            return Err(reason);
        }
    }
    database
        .refresh_index_dir_stats(&dir_path)
        .await
        .map_err(|error| error.to_string())?;

    database
        .get_index_status()
        .await
        .map_err(|error| error.to_string())
}

async fn rebuild_paths(
    database: &Database,
    dir_paths: &[String],
) -> Result<IndexStatusView, sqlx::Error> {
    if dir_paths.is_empty() {
        return database.get_index_status().await;
    }

    let settings = database.get_index_settings().await?;
    let discovered = scanner::discover_supported_files_with_settings(dir_paths, &settings);
    let discovered_by_path = discovered
        .iter()
        .map(|file| (file.path.to_string_lossy().to_string(), file.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    let mut existing_by_path = HashMap::<String, DocumentState>::new();
    for dir_path in dir_paths {
        for state in database.document_states_in_dir(dir_path).await? {
            existing_by_path.insert(state.path.clone(), state);
        }
    }

    let mut to_delete = Vec::new();
    for path in existing_by_path.keys() {
        if !discovered_by_path.contains_key(path) {
            to_delete.push(path.clone());
        }
    }

    let mut to_update = Vec::new();
    for file in discovered_by_path.values() {
        let path = file.path.to_string_lossy().to_string();
        let needs_update = match existing_by_path.get(&path) {
            Some(state) => {
                state.file_size != file.file_size
                    || state.modified_at != file.modified_at
                    || state.content_hash != file.content_hash
            }
            None => true,
        };

        if needs_update {
            to_update.push(file.clone());
        }
    }

    let total = to_delete.len() + to_update.len();
    let skipped = discovered.len().saturating_sub(total);
    let plan = IndexPlan {
        dir_paths: dir_paths.to_vec(),
        pending_delete_paths: to_delete.into_iter().collect(),
        pending_update_paths: to_update
            .into_iter()
            .map(|file| file.path.to_string_lossy().to_string())
            .collect(),
        total,
        processed: 0,
        succeeded: 0,
        failed: 0,
        updated: 0,
        skipped,
        deleted: 0,
    };

    process_index_plan(database, plan, &settings, &existing_by_path).await
}

struct IndexPlan {
    dir_paths: Vec<String>,
    pending_delete_paths: VecDeque<String>,
    pending_update_paths: VecDeque<String>,
    total: usize,
    processed: usize,
    succeeded: usize,
    failed: usize,
    updated: usize,
    skipped: usize,
    deleted: usize,
}

async fn process_index_plan(
    database: &Database,
    mut plan: IndexPlan,
    settings: &crate::docmind::storage::types::IndexSettings,
    existing_by_path: &HashMap<String, DocumentState>,
) -> Result<IndexStatusView, sqlx::Error> {
    trace_indexer(&format!(
        "process_index_plan dirs={:?} delete={} update={} total={} processed={}",
        plan.dir_paths,
        plan.pending_delete_paths.len(),
        plan.pending_update_paths.len(),
        plan.total,
        plan.processed
    ));

    if plan.total == 0 {
        database
            .save_index_run_summary(
                plan.updated,
                plan.skipped,
                plan.deleted,
                plan.processed,
                0,
                plan.succeeded,
                plan.failed,
            )
            .await?;
        for dir_path in &plan.dir_paths {
            database
                .set_index_dir_status(
                    dir_path,
                    existing_count_for_dir(existing_by_path, dir_path),
                    existing_chunks_for_dir(database, dir_path).await.unwrap_or(0),
                    "indexed",
                )
                .await?;
        }
        database.clear_current_task().await?;
        database.clear_index_checkpoint().await?;
        return database.get_index_status().await;
    }

    database
        .set_current_task(
            "正在重新索引本地文档",
            "扫描并提取可搜索文本",
            "running",
            "",
            "",
            progress_of(plan.processed, plan.total),
            plan.processed,
            plan.total,
            plan.succeeded,
            plan.failed,
            plan.updated,
            plan.skipped,
            plan.deleted,
            false,
        )
        .await?;

    for dir_path in &plan.dir_paths {
        database
            .set_index_dir_status(
                dir_path,
                existing_count_for_dir(existing_by_path, dir_path),
                existing_chunks_for_dir(database, dir_path).await.unwrap_or(0),
                "indexing",
            )
            .await?;
    }

    save_checkpoint(database, &plan, "delete", "", "").await?;

    while let Some(path) = plan.pending_delete_paths.pop_front() {
        if database.current_task_pause_requested().await? {
            save_checkpoint(database, &plan, "delete", "", "").await?;
            return pause_current_task(
                database,
                "正在重新索引本地文档",
                "任务已暂停，等待恢复",
                "paused",
                "",
                "",
                plan.processed,
                plan.total,
                plan.succeeded,
                plan.failed,
                plan.updated,
                plan.skipped,
                plan.deleted,
            )
            .await;
        }

        plan.processed += 1;
        let progress = progress_of(plan.processed, plan.total);
        let current_dir = resolve_dir_for_path(&plan.dir_paths, &path);

        database
            .set_current_task(
                "正在重新索引本地文档",
                "清理已删除文件",
                "running",
                &current_dir,
                &path,
                progress,
                plan.processed,
                plan.total,
                plan.succeeded,
                plan.failed,
                plan.updated,
                plan.skipped,
                plan.deleted,
                false,
            )
            .await?;

        match database.clear_document_by_path(&path).await {
            Ok(()) => {
                let _ = database.clear_failed_file(&path).await;
                plan.succeeded += 1;
                plan.deleted += 1;
            }
            Err(error) => {
                plan.failed += 1;
                let reason = error.to_string();
                let (category, code) = classify_failure(&reason, &path);
                let _ = database
                    .record_failed_file(&path, &reason, &category, &code)
                    .await;
            }
        }

        save_checkpoint(database, &plan, "delete", &current_dir, &path).await?;
    }

    while let Some(path) = plan.pending_update_paths.pop_front() {
        let current_dir = resolve_dir_for_path(&plan.dir_paths, &path);

        if database.current_task_pause_requested().await? {
            save_checkpoint(database, &plan, "update", &current_dir, &path).await?;
            return pause_current_task(
                database,
                "正在重新索引本地文档",
                "任务已暂停，等待恢复",
                "paused",
                &current_dir,
                &path,
                plan.processed,
                plan.total,
                plan.succeeded,
                plan.failed,
                plan.updated,
                plan.skipped,
                plan.deleted,
            )
            .await;
        }

        plan.processed += 1;
        let progress = progress_of(plan.processed, plan.total);

        database
            .set_current_task(
                "正在重新索引本地文档",
                &path,
                "running",
                &current_dir,
                &path,
                progress,
                plan.processed,
                plan.total,
                plan.succeeded,
                plan.failed,
                plan.updated,
                plan.skipped,
                plan.deleted,
                false,
            )
            .await?;

        let file = scanner::snapshot_supported_file(&current_dir, Path::new(&path), settings)
            .map_err(sqlx::Error::Protocol)?;
        match scanner::parse_document(&file) {
            Ok((document, chunks)) => {
                match database.store_document(&document, &chunks).await {
                    Ok(()) => {
                        plan.succeeded += 1;
                        plan.updated += 1;
                        let _ = database.refresh_index_dir_stats(&file.dir_path).await;
                        let _ = database.clear_failed_file(&path).await;
                    }
                    Err(error) => {
                        plan.failed += 1;
                        let reason = error.to_string();
                        let (category, code) = classify_failure(&reason, &path);
                        let _ = database
                            .record_failed_file(&path, &reason, &category, &code)
                            .await;
                    }
                }
            }
            Err(reason) => {
                plan.failed += 1;
                let (category, code) = classify_failure(&reason, &path);
                let _ = database
                    .record_failed_file(&path, &reason, &category, &code)
                    .await;
            }
        }

        save_checkpoint(database, &plan, "update", &current_dir, &path).await?;
    }

    database
        .save_index_run_summary(
            plan.updated,
            plan.skipped,
            plan.deleted,
            plan.processed,
            plan.total,
            plan.succeeded,
            plan.failed,
        )
        .await?;
    database.clear_index_checkpoint().await?;

    for dir_path in &plan.dir_paths {
        database.refresh_index_dir_stats(dir_path).await?;
    }

    database.clear_current_task().await?;
    trace_indexer("process_index_plan done");
    database.get_index_status().await
}

async fn save_checkpoint(
    database: &Database,
    plan: &IndexPlan,
    phase: &str,
    current_dir: &str,
    current_file: &str,
) -> Result<(), sqlx::Error> {
    database
        .save_index_checkpoint(
            &plan.dir_paths,
            &plan.pending_delete_paths.iter().cloned().collect::<Vec<_>>(),
            &plan.pending_update_paths.iter().cloned().collect::<Vec<_>>(),
            phase,
            current_dir,
            current_file,
            plan.total,
            plan.processed,
            plan.succeeded,
            plan.failed,
            plan.updated,
            plan.skipped,
            plan.deleted,
        )
        .await
}

async fn pause_current_task(
    database: &Database,
    label: &str,
    details: &str,
    state: &str,
    current_dir: &str,
    current_file: &str,
    scanned: usize,
    total: usize,
    succeeded: usize,
    failed: usize,
    updated: usize,
    skipped: usize,
    deleted: usize,
) -> Result<IndexStatusView, sqlx::Error> {
    database
        .set_current_task(
            label,
            details,
            state,
            current_dir,
            current_file,
            progress_of(scanned, total),
            scanned,
            total,
            succeeded,
            failed,
            updated,
            skipped,
            deleted,
            true,
        )
        .await?;
    database.get_index_status().await
}

fn progress_of(processed: usize, total: usize) -> u8 {
    if total == 0 {
        100
    } else {
        ((processed as f32 / total as f32) * 100.0).round() as u8
    }
}

fn resolve_dir_for_path(dir_paths: &[String], path: &str) -> String {
    dir_paths
        .iter()
        .filter(|candidate| path.starts_with(candidate.as_str()))
        .max_by_key(|candidate| candidate.len())
        .cloned()
        .unwrap_or_default()
}

fn existing_count_for_dir(existing: &std::collections::HashMap<String, DocumentState>, dir_path: &str) -> usize {
    existing
        .values()
        .filter(|state| state.path.starts_with(dir_path))
        .count()
}

async fn existing_chunks_for_dir(database: &Database, dir_path: &str) -> Result<usize, sqlx::Error> {
    Ok(database
        .list_documents_in_dir(dir_path)
        .await?
        .into_iter()
        .map(|doc| doc.chunks)
        .sum())
}

fn trace_indexer(message: &str) {
    if std::env::var("DOCMIND_TRACE_INDEXER").is_ok() {
        eprintln!("[docmind:indexer] {message}");
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_dir_processes_real_markdown_directory_in_isolated_home() {
        let temp_home = std::env::temp_dir().join("docmind-indexer-debug-home");
        if temp_home.exists() {
            std::fs::remove_dir_all(&temp_home).expect("cleanup temp home");
        }
        std::fs::create_dir_all(&temp_home).expect("create temp home");
        std::env::set_var("HOME", &temp_home);

        let dir_path = "/Users/zhaoyang/Documents/MarkdownHome/zhaoyang-markdown/AI/面向agent编程";

        let result = tauri::async_runtime::block_on(async {
            let database = Database::open_or_init().await.expect("open temp database");
            database
                .add_index_dir(dir_path)
                .await
                .expect("add dir");
            rebuild_dir(&database, dir_path).await.expect("rebuild dir")
        });

        assert_eq!(result.indexed_docs, 4);
        assert_eq!(result.failed_files, 0);
        assert!(result.current_task.is_none());
    }
}
