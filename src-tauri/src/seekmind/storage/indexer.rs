/*
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 索引流程编排与任务执行。
 */
#![allow(dead_code)]

use crate::seekmind::models::{IndexRefreshProgressView, IndexStatusView};
use crate::seekmind::parser::types::ParserStreamEvent;
use crate::seekmind::storage::scanner;
use crate::seekmind::storage::types::{DocumentState, IndexSettings, ParserSource};
use crate::seekmind::storage::Database;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{mpsc, Arc};
use std::thread;

type IndexProgressEmitter = Arc<dyn Fn(IndexRefreshProgressView) + Send + Sync>;

pub async fn rebuild_all(
    database: &Database,
    job_id: &str,
    on_progress: IndexProgressEmitter,
) -> Result<IndexStatusView, sqlx::Error> {
    database.clear_pause_request().await?;
    database.clear_index_checkpoint().await?;
    let dir_paths = database.enabled_index_dir_paths().await?;
    database.clear_failed_files().await?;
    database.clear_current_task().await?;
    rebuild_paths(database, &dir_paths, job_id, "all", on_progress).await
}

pub async fn rebuild_dir(
    database: &Database,
    dir_path: &str,
    job_id: &str,
    on_progress: IndexProgressEmitter,
) -> Result<IndexStatusView, sqlx::Error> {
    database.clear_pause_request().await?;
    database.clear_index_checkpoint().await?;
    let dir_paths = vec![dir_path.to_string()];
    rebuild_paths(database, &dir_paths, job_id, "dir", on_progress).await
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
        serde_json::from_str(&checkpoint.pending_delete_paths)
            .map_err(|error| error.to_string())?;
    let pending_update_paths: VecDeque<String> =
        serde_json::from_str(&checkpoint.pending_update_paths)
            .map_err(|error| error.to_string())?;

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

    process_index_plan(
        database,
        plan,
        &settings,
        &existing_by_path,
        "",
        "resume",
        Arc::new(|_: IndexRefreshProgressView| {}),
    )
    .await
    .map_err(|error| error.to_string())
}

pub async fn retry_failed_file(database: &Database, path: &str) -> Result<IndexStatusView, String> {
    let settings = database
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    match reparse_document_once(database, path, &settings).await {
        Ok((_, warning)) => {
            if let Some(warning) = warning {
                eprintln!("[SeekMind] retry_failed_file warning path={path}: {warning}");
            }
        }
        Err(reason) => return Err(reason),
    }
    database
        .get_index_status()
        .await
        .map_err(|error| error.to_string())
}

pub async fn rebuild_pdf_ocr_queue(
    database: &Database,
    job_id: &str,
    on_progress: IndexProgressEmitter,
) -> Result<IndexStatusView, String> {
    let pending_paths = database
        .pending_pdf_ocr_document_paths()
        .await
        .map_err(|error| error.to_string())?;

    if pending_paths.is_empty() {
        return database
            .get_index_status()
            .await
            .map_err(|error| error.to_string());
    }

    let settings = database
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    let dir_paths = database
        .enabled_index_dir_paths()
        .await
        .map_err(|error| error.to_string())?;
    let total = pending_paths.len();
    let mut processed = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut updated = 0usize;

    database
        .set_current_task(
            "正在重跑 PDF OCR",
            "重新处理扫描版 PDF 识别任务",
            "running",
            "",
            "",
            0,
            0,
            total,
            0,
            0,
            0,
            0,
            0,
            None,
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    emit_index_progress(database, job_id, "pdf-ocr", "", "running", "正在重跑 PDF OCR", &on_progress)
        .await
        .map_err(|error| error.to_string())?;

    for path in pending_paths {
        processed += 1;
        let progress = progress_of(processed, total);
        let dir_path = resolve_dir_for_path(&dir_paths, &path);
        let current_message = format!("正在重跑 PDF OCR · {processed}/{total}");
        database
            .set_current_task(
                "正在重跑 PDF OCR",
                &current_message,
                "running",
                &dir_path,
                &path,
                progress,
                processed,
                total,
                succeeded,
                failed,
                updated,
                0,
                0,
                None,
                false,
            )
            .await
            .map_err(|error| error.to_string())?;

        match reparse_document_once(database, &path, &settings).await {
            Ok((parsed_dir, warning)) => {
                let warning_text = warning.clone();
                if let Some(warning) = warning_text {
                    eprintln!("[SeekMind] pdf ocr warning path={path}: {warning}");
                }
                succeeded += 1;
                updated += 1;
                let status = database
                    .get_index_status()
                    .await
                    .map_err(|error| error.to_string())?;
                let message = if let Some(warning) = warning {
                    format!("PDF OCR 重跑完成，存在提示：{warning}")
                } else {
                    "PDF OCR 重跑完成".to_string()
                };
                on_progress(IndexRefreshProgressView {
                    job_id: job_id.to_string(),
                    state: "running".to_string(),
                    message,
                    scope: "pdf-ocr".to_string(),
                    path: path.clone(),
                    parser_source: String::new(),
                    warning: None,
                    status,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                });
                if !parsed_dir.trim().is_empty() {
                    let _ = database.refresh_index_dir_stats(&parsed_dir).await;
                }
            }
            Err(error) => {
                failed += 1;
                let status = database
                    .get_index_status()
                    .await
                    .map_err(|db_error| db_error.to_string())?;
                let (category, code) = classify_failure(&error, &path);
                let _ = database
                    .record_failed_file(&path, &error, &category, &code)
                    .await;
                on_progress(IndexRefreshProgressView {
                    job_id: job_id.to_string(),
                    state: "running".to_string(),
                    message: format!("PDF OCR 重跑失败：{error}"),
                    scope: "pdf-ocr".to_string(),
                    path: path.clone(),
                    parser_source: String::new(),
                    warning: None,
                    status,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                });
            }
        }
    }

    database
        .set_current_task(
            "PDF OCR 重跑完成",
            "扫描版 PDF 识别任务已处理完成",
            "completed",
            "",
            "",
            100,
            processed,
            total,
            succeeded,
            failed,
            updated,
            0,
            0,
            None,
            false,
        )
        .await
        .map_err(|error| error.to_string())?;
    let status = database
        .get_index_status()
        .await
        .map_err(|error| error.to_string())?;
    on_progress(IndexRefreshProgressView {
        job_id: job_id.to_string(),
        state: "completed".to_string(),
        message: "PDF OCR 队列重跑完成".to_string(),
        scope: "pdf-ocr".to_string(),
        path: String::new(),
        parser_source: String::new(),
        warning: None,
        status: status.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    });
    database
        .clear_current_task()
        .await
        .map_err(|error| error.to_string())?;
    Ok(status)
}

async fn reparse_document_once(
    database: &Database,
    path: &str,
    settings: &IndexSettings,
) -> Result<(String, Option<String>), String> {
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

    let file = scanner::snapshot_supported_file(&dir_path, path_buf, settings)?;
    match scanner::parse_document(&file) {
        Ok((document, chunks, blocks, ocr_tasks, outcome)) => {
            if let Some(ref warning) = outcome.warning {
                eprintln!("[SeekMind] reparse_document_once warning path={normalized}: {warning}");
            }
            database
                .clear_document_by_path(normalized)
                .await
                .map_err(|error| error.to_string())?;
            database
                .store_document(&document, &chunks, &blocks, &ocr_tasks)
                .await
                .map_err(|error| error.to_string())?;
            database
                .clear_failed_file(normalized)
                .await
                .map_err(|error| error.to_string())?;
            database
                .refresh_index_dir_stats(&dir_path)
                .await
                .map_err(|error| error.to_string())?;
            Ok((dir_path, outcome.warning))
        }
        Err(reason) => {
            let (category, code) = classify_failure(&reason, normalized);
            database
                .record_failed_file(normalized, &reason, &category, &code)
                .await
                .map_err(|error| error.to_string())?;
            Err(reason)
        }
    }
}

async fn rebuild_paths(
    database: &Database,
    dir_paths: &[String],
    job_id: &str,
    scope: &str,
    on_progress: IndexProgressEmitter,
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

    process_index_plan(
        database,
        plan,
        &settings,
        &existing_by_path,
        job_id,
        scope,
        on_progress,
    )
    .await
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
    settings: &crate::seekmind::storage::types::IndexSettings,
    existing_by_path: &HashMap<String, DocumentState>,
    job_id: &str,
    scope: &str,
    on_progress: IndexProgressEmitter,
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
                    existing_chunks_for_dir(database, dir_path)
                        .await
                        .unwrap_or(0),
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
            None,
            false,
        )
        .await?;
    emit_index_progress(
        database,
        job_id,
        scope,
        "",
        "running",
        "正在重新索引本地文档",
        &on_progress,
    )
    .await?;

    for dir_path in &plan.dir_paths {
        database
            .set_index_dir_status(
                dir_path,
                existing_count_for_dir(existing_by_path, dir_path),
                existing_chunks_for_dir(database, dir_path)
                    .await
                    .unwrap_or(0),
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
                job_id,
                scope,
                &on_progress,
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
                None,
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
        emit_index_progress(
            database,
            job_id,
            scope,
            &path,
            "running",
            "已处理删除文件",
            &on_progress,
        )
        .await?;
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
                job_id,
                scope,
                &on_progress,
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
                None,
                false,
            )
            .await?;

        let file = scanner::snapshot_supported_file(&current_dir, Path::new(&path), settings)
            .map_err(sqlx::Error::Protocol)?;
        emit_index_progress(
            database,
            job_id,
            scope,
            &path,
            "running",
            "正在解析并索引文档",
            &on_progress,
        )
        .await?;
        let processed_before_current = plan.processed.saturating_sub(1);
        let (parser_tx, parser_thread) = spawn_parser_progress_forwarder(
            database.clone(),
            on_progress.clone(),
            job_id.to_string(),
            scope.to_string(),
            current_dir.clone(),
            path.clone(),
            plan.processed,
            plan.total,
            processed_before_current,
            plan.succeeded,
            plan.failed,
            plan.updated,
            plan.skipped,
            plan.deleted,
            "正在解析并索引文档".to_string(),
        );
        let parser_tx_for_parse = parser_tx.clone();
        eprintln!("[SeekMind] indexing parse start path={}", path);
        let parse_result = scanner::parse_document_with_progress(&file, move |event| {
            let _ = parser_tx_for_parse.send(event);
        });
        drop(parser_tx);
        let _ = parser_thread.join();

        let actual_parser_source: Option<String>;
        let mut actual_parser_warning: Option<String> = None;
        match parse_result {
            Ok((document, chunks, blocks, ocr_tasks, outcome)) => {
                actual_parser_source = Some(match outcome.source {
                    ParserSource::Python => "python".to_string(),
                    ParserSource::Rust => "rust".to_string(),
                });
                actual_parser_warning = outcome.warning.clone();
                eprintln!(
                    "[SeekMind] indexing parse completed path={} route={} chunks={}",
                    path,
                    parser_route_label(actual_parser_source.as_deref()),
                    chunks.len()
                );
                if let Some(warning) = actual_parser_warning.clone() {
                    eprintln!("[SeekMind] indexing warning for {}: {warning}", path);
                }
                match database
                    .store_document(&document, &chunks, &blocks, &ocr_tasks)
                    .await
                {
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
                actual_parser_source = Some("rust".to_string());
                let (category, code) = classify_failure(&reason, &path);
                let _ = database
                    .record_failed_file(&path, &reason, &category, &code)
                    .await;
            }
        }

        save_checkpoint(database, &plan, "update", &current_dir, &path).await?;
        emit_index_progress(
            database,
            job_id,
            scope,
            &path,
            "running",
            "已处理文档",
            &on_progress,
        )
        .await?;
        if let Some(actual_parser_source) = actual_parser_source {
            let status_snapshot = database.get_index_status().await?;
            on_progress(IndexRefreshProgressView {
                job_id: job_id.to_string(),
                state: "running".to_string(),
                message: String::new(),
                scope: scope.to_string(),
                path: path.clone(),
                parser_source: actual_parser_source,
                warning: actual_parser_warning,
                status: status_snapshot,
                updated_at: chrono::Utc::now().to_rfc3339(),
            });
        }
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
            &plan
                .pending_delete_paths
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            &plan
                .pending_update_paths
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
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
    job_id: &str,
    scope: &str,
    on_progress: &IndexProgressEmitter,
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
            None,
            true,
        )
        .await?;
    emit_index_progress(
        database,
        job_id,
        scope,
        current_file,
        "paused",
        details,
        on_progress,
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

fn existing_count_for_dir(
    existing: &std::collections::HashMap<String, DocumentState>,
    dir_path: &str,
) -> usize {
    existing
        .values()
        .filter(|state| state.path.starts_with(dir_path))
        .count()
}

async fn existing_chunks_for_dir(
    database: &Database,
    dir_path: &str,
) -> Result<usize, sqlx::Error> {
    Ok(database
        .list_documents_in_dir(dir_path)
        .await?
        .into_iter()
        .map(|doc| doc.chunks)
        .sum())
}

async fn emit_index_progress(
    database: &Database,
    job_id: &str,
    scope: &str,
    path: &str,
    state: &str,
    message: &str,
    on_progress: &IndexProgressEmitter,
) -> Result<(), sqlx::Error> {
    let status = database.get_index_status().await?;
    on_progress(IndexRefreshProgressView {
        job_id: job_id.to_string(),
        state: state.to_string(),
        message: message.to_string(),
        scope: scope.to_string(),
        path: path.to_string(),
        parser_source: String::new(),
        warning: None,
        status,
        updated_at: chrono::Utc::now().to_rfc3339(),
    });
    Ok(())
}

fn spawn_parser_progress_forwarder(
    database: Database,
    on_progress: IndexProgressEmitter,
    job_id: String,
    scope: String,
    current_dir: String,
    path: String,
    scanned: usize,
    total: usize,
    processed_before_current: usize,
    succeeded: usize,
    failed: usize,
    updated: usize,
    skipped: usize,
    deleted: usize,
    fallback_message: String,
) -> (mpsc::Sender<ParserStreamEvent>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<ParserStreamEvent>();

    let handle = thread::spawn(move || {
        let mut last_warning: Option<String> = None;
        while let Ok(event) = rx.recv() {
            if let Some(warning) = event.warning.clone() {
                last_warning = Some(warning);
            }
            let details = parser_event_message(&event, &fallback_message);
            let progress = progress_with_stream(processed_before_current, total, event.percent);
            let database = database.clone();
            let on_progress = on_progress.clone();
            let job_id = job_id.clone();
            let scope = scope.clone();
            let current_dir = current_dir.clone();
            let path = path.clone();
            let details_for_task = details.clone();
            let warning_for_task = last_warning.clone();
            let _ = tauri::async_runtime::block_on(async move {
                let _ = database
                    .set_current_task(
                        "正在重新索引本地文档",
                        &details_for_task,
                        "running",
                        &current_dir,
                        &path,
                        progress,
                        scanned,
                        total,
                        succeeded,
                        failed,
                        updated,
                        skipped,
                        deleted,
                        warning_for_task.as_deref(),
                        false,
                    )
                    .await;

                if let Ok(status) = database.get_index_status().await {
                    on_progress(IndexRefreshProgressView {
                        job_id,
                        state: "running".to_string(),
                        message: details_for_task,
                        scope,
                        path,
                        parser_source: event.parser_source,
                        warning: warning_for_task,
                        status,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    });
                }
            });
        }
    });

    (tx, handle)
}

fn progress_with_stream(processed_before_current: usize, total: usize, event_percent: u8) -> u8 {
    if total == 0 {
        return 0;
    }

    let completed = processed_before_current as f32 + (event_percent as f32 / 100.0);
    ((completed / total as f32) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

fn parser_event_message(event: &ParserStreamEvent, fallback: &str) -> String {
    if !event.message.trim().is_empty() {
        if event.stage == "chunk" && !event.current.trim().is_empty() {
            return format!("{} · {}", event.message.trim(), event.current.trim());
        }
        return event.message.trim().to_string();
    }

    match event.stage.as_str() {
        "start" => "正在开始解析".to_string(),
        "extract" => "正在提取内容块".to_string(),
        "normalize" => "正在整理内容块".to_string(),
        "ocr_queue" => {
            if !event.current.trim().is_empty() && event.total > 0 {
                format!("正在排队 OCR · {}/{}", event.processed, event.total)
            } else {
                "正在排队 OCR".to_string()
            }
        }
        "chunk" => {
            if !event.current.trim().is_empty() {
                format!("正在生成切片 · {}", event.current.trim())
            } else {
                "正在生成切片".to_string()
            }
        }
        "done" => "解析完成".to_string(),
        _ => fallback.to_string(),
    }
}

fn trace_indexer(message: &str) {
    if std::env::var("SeekMind_TRACE_INDEXER").is_ok() {
        eprintln!("[seekmind:indexer] {message}");
    }
}

fn parser_route_label(source: Option<&str>) -> &'static str {
    // 修复：索引日志只保留用户可读的链路称呼，避免把内部实现名直接写到控制台输出里。
    match source {
        Some("python") => "default",
        Some("rust") => "fallback",
        _ => "unknown",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_dir_processes_real_markdown_directory_in_isolated_home() {
        let temp_home = std::env::temp_dir().join("seekmind-indexer-debug-home");
        if temp_home.exists() {
            std::fs::remove_dir_all(&temp_home).expect("cleanup temp home");
        }
        std::fs::create_dir_all(&temp_home).expect("create temp home");
        std::env::set_var("HOME", &temp_home);

        let dir_path = "/Users/zhaoyang/Documents/MarkdownHome/zhaoyang-markdown/AI/面向agent编程";

        let result = tauri::async_runtime::block_on(async {
            let database = Database::open_or_init().await.expect("open temp database");
            database.add_index_dir(dir_path).await.expect("add dir");
            rebuild_dir(&database, dir_path, "test", Arc::new(|_| {}))
                .await
                .expect("rebuild dir")
        });

        assert_eq!(result.indexed_docs, 4);
        assert_eq!(result.failed_files, 0);
        assert!(result.current_task.is_none());
    }
}
