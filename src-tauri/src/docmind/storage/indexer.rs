#![allow(dead_code)]

use crate::docmind::models::IndexStatusView;
use crate::docmind::storage::scanner;
use crate::docmind::storage::Database;

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

    let document = scanner::extract_document_at(&dir_path, path_buf)?;
    let chunks = scanner::chunk_document(&document);

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

    database
        .set_current_task(
            "正在重新索引本地文档",
            "扫描并提取可搜索文本",
            0,
            0,
            total,
        )
        .await?;

    for dir_path in dir_paths {
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

        database
            .set_current_task(
                "正在重新索引本地文档",
                &file.path.to_string_lossy(),
                progress,
                processed,
                total,
            )
            .await?;

        match scanner::extract_document(&file) {
            Ok(document) => {
                let chunks = scanner::chunk_document(&document);
                database.store_document(&document, &chunks).await?;
            }
            Err(reason) => {
                failed_items.push((file.path.to_string_lossy().to_string(), reason));
            }
        }
    }

    database.replace_failed_files(&failed_items).await?;

    for dir_path in dir_paths {
        database.refresh_index_dir_stats(dir_path).await?;
    }

    database.clear_current_task().await?;
    database.get_index_status().await
}
