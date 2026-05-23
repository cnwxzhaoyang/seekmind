#![allow(dead_code)]

use crate::docmind::models::IndexStatusView;
use crate::docmind::storage::scanner;
use crate::docmind::storage::Database;
use crate::docmind::storage::types::DiscoveredFile;

pub async fn rebuild_all(database: &Database) -> Result<IndexStatusView, sqlx::Error> {
    let dir_paths = database.enabled_index_dir_paths().await?;
    database.clear_failed_files().await?;
    database.clear_current_task().await?;
    rebuild_paths(database, &dir_paths).await
}

pub async fn rebuild_dir(database: &Database, dir_path: &str) -> Result<IndexStatusView, sqlx::Error> {
    let dir_paths = vec![dir_path.to_string()];
    rebuild_paths(database, &dir_paths).await
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

    let file = DiscoveredFile {
        dir_path: dir_path.clone(),
        path: path_buf.to_path_buf(),
    };
    let (document, chunks) = scanner::parse_document(&file)?;

    database
        .clear_document_by_path(normalized)
        .await
        .map_err(|error| error.to_string())?;
    database
        .clear_failed_file(normalized)
        .await
        .map_err(|error| error.to_string())?;
    database
        .store_document(&document, &chunks)
        .await
        .map_err(|error| error.to_string())?;
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

    let discovered = scanner::discover_supported_files(dir_paths);
    let total = discovered.len();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    trace_indexer(&format!(
        "rebuild_paths start dirs={:?} discovered={total}",
        dir_paths
    ));

    database
        .set_current_task(
            "正在重新索引本地文档",
            "扫描并提取可搜索文本",
            "",
            "",
            0,
            0,
            total,
            succeeded,
            failed,
        )
        .await?;

    for dir_path in dir_paths {
        trace_indexer(&format!("clearing directory data: {dir_path}"));
        database.clear_directory_documents(dir_path).await?;
        database.clear_directory_failed_files(dir_path).await?;
        database
            .set_index_dir_status(dir_path, 0, 0, "indexing")
            .await?;
    }

    let mut failed_items = Vec::new();
    let mut processed = 0usize;

    for file in discovered {
        processed += 1;
        let progress = if total == 0 {
            100
        } else {
            ((processed as f32 / total as f32) * 100.0).round() as u8
        };
        let current_dir = file.dir_path.clone();
        let current_file = file.path.to_string_lossy().to_string();

        trace_indexer(&format!(
            "processing file {processed}/{total}: dir={current_dir} file={current_file}"
        ));

        database
            .set_current_task(
                "正在重新索引本地文档",
                &current_file,
                &current_dir,
                &current_file,
                progress,
                processed,
                total,
                succeeded,
                failed,
            )
            .await?;

        match scanner::parse_document(&file) {
            Ok((document, chunks)) => {
                trace_indexer(&format!(
                    "store_document begin: file={current_file} chunks={}",
                    chunks.len()
                ));
                match database.store_document(&document, &chunks).await {
                    Ok(()) => {
                        succeeded += 1;
                        trace_indexer(&format!("store_document ok: file={current_file}"));
                        let _ = database.refresh_index_dir_stats(&file.dir_path).await;
                    }
                    Err(error) => {
                        failed += 1;
                        trace_indexer(&format!(
                            "store_document error: file={current_file} err={error}"
                        ));
                        failed_items.push((file.path.to_string_lossy().to_string(), error.to_string()));
                    }
                }
            }
            Err(reason) => {
                failed += 1;
                trace_indexer(&format!("extract error: file={current_file} err={reason}"));
                failed_items.push((file.path.to_string_lossy().to_string(), reason));
            }
        }

        database
            .set_current_task(
                "正在重新索引本地文档",
                &current_file,
                &current_dir,
                &current_file,
                progress,
                processed,
                total,
                succeeded,
                failed,
            )
            .await?;
    }

    database.replace_failed_files(&failed_items).await?;

    for dir_path in dir_paths {
        database.refresh_index_dir_stats(dir_path).await?;
    }

    database.clear_current_task().await?;
    trace_indexer("rebuild_paths done");
    database.get_index_status().await
}

fn trace_indexer(message: &str) {
    if std::env::var("DOCMIND_TRACE_INDEXER").is_ok() {
        eprintln!("[docmind:indexer] {message}");
    }
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
