/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 文档、块、OCR 任务与文档入口相关数据库逻辑。
 */

use std::path::{Path, PathBuf};

use sqlx::Row;
use uuid::Uuid;

use crate::seekmind::file_ops;
use crate::seekmind::models::{ChunkView, DocumentView, IndexStatusView, PreviewBlockView};
use crate::seekmind::parser::types::PdfOcrTask;
use crate::seekmind::storage::types::{ChunkRecord, DocumentState, ExtractedDocument};

use super::rows::{BlockRow, ChunkRow, DocumentRow};
use super::util::{current_unix_ts, is_virtual_directory, scalar_count_no_bind};
use super::Database;

impl Database {
    pub async fn open_file(&self, path: &str) -> Result<(), String> {
        let result = file_ops::open_file_path(path);
        if result.is_ok() {
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(path);
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            let title = std::path::Path::new(path)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(file_name);
            let _ = self
                .record_recent_document(path, title, file_name, ext)
                .await;
        }
        result
    }

    pub async fn quick_look_file(&self, app: &tauri::AppHandle, path: &str) -> Result<(), String> {
        file_ops::quick_look_file_path(app, path)
    }

    pub async fn list_documents_in_dir(
        &self,
        dir_path: &str,
    ) -> Result<Vec<DocumentView>, sqlx::Error> {
        let rows = if is_virtual_directory(dir_path) {
            sqlx::query_as::<_, DocumentRow>(
                r#"
                SELECT
                    d.id,
                    d.dir_path,
                    d.path,
                    d.file_name,
                    d.ext,
                    d.modified,
                    COUNT(c.id) AS chunks
                FROM documents d
                LEFT JOIN chunks c ON c.document_id = d.id
                WHERE d.dir_path = ?
                GROUP BY d.id, d.dir_path, d.path, d.file_name, d.ext, d.modified
                ORDER BY d.path
                "#,
            )
            .bind(dir_path)
            .fetch_all(&self.pool)
            .await?
        } else {
            let prefix = format!("{dir_path}/%");
            sqlx::query_as::<_, DocumentRow>(
                r#"
                SELECT
                    d.id,
                    d.dir_path,
                    d.path,
                    d.file_name,
                    d.ext,
                    d.modified,
                    COUNT(c.id) AS chunks
                FROM documents d
                LEFT JOIN chunks c ON c.document_id = d.id
                WHERE d.path = ? OR d.path LIKE ?
                GROUP BY d.id, d.dir_path, d.path, d.file_name, d.ext, d.modified
                ORDER BY d.path
                "#,
            )
            .bind(dir_path)
            .bind(prefix)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(|row| DocumentView {
                id: row.id,
                dir_path: row.dir_path,
                path: row.path,
                file_name: row.file_name,
                ext: row.ext,
                modified: row.modified,
                chunks: row.chunks as usize,
            })
            .collect())
    }

    pub(crate) async fn document_states_in_dir(
        &self,
        dir_path: &str,
    ) -> Result<Vec<DocumentState>, sqlx::Error> {
        #[derive(Debug, sqlx::FromRow)]
        struct DocumentStateRow {
            path: String,
            file_size: i64,
            modified_at: i64,
            content_hash: String,
        }

        let rows = if is_virtual_directory(dir_path) {
            sqlx::query_as::<_, DocumentStateRow>(
                r#"
                SELECT path, file_size, modified_at, content_hash
                FROM documents
                WHERE dir_path = ?
                "#,
            )
            .bind(dir_path)
            .fetch_all(&self.pool)
            .await?
        } else {
            let prefix = format!("{dir_path}/%");
            sqlx::query_as::<_, DocumentStateRow>(
                r#"
                SELECT path, file_size, modified_at, content_hash
                FROM documents
                WHERE path = ? OR path LIKE ?
                "#,
            )
            .bind(dir_path)
            .bind(prefix)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows
            .into_iter()
            .map(|row| DocumentState {
                path: row.path,
                file_size: row.file_size,
                modified_at: row.modified_at,
                content_hash: row.content_hash,
            })
            .collect())
    }

    fn resolve_preview_asset_path(asset_path: &str, document_path: &str) -> String {
        let cleaned = asset_path.trim();
        if cleaned.is_empty() {
            return String::new();
        }
        if cleaned.starts_with("http://")
            || cleaned.starts_with("https://")
            || cleaned.starts_with("data:")
            || cleaned.starts_with("blob:")
            || cleaned.starts_with("file:")
            || Path::new(cleaned).is_absolute()
        {
            return cleaned.to_string();
        }
        let base = Path::new(document_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(document_path));
        base.join(cleaned).to_string_lossy().into_owned()
    }

    fn log_preview_image_block(
        document_path: &str,
        raw_asset_path: &str,
        resolved_asset_path: &str,
    ) {
        let exists = if resolved_asset_path.starts_with("http://")
            || resolved_asset_path.starts_with("https://")
            || resolved_asset_path.starts_with("data:")
            || resolved_asset_path.starts_with("blob:")
        {
            "remote".to_string()
        } else {
            Path::new(resolved_asset_path).exists().to_string()
        };
        eprintln!(
            "[SeekMind] preview image block document={} raw_asset_path={} resolved_asset_path={} exists={}",
            document_path, raw_asset_path, resolved_asset_path, exists
        );
    }

    pub(crate) fn build_preview_block(
        document_path: &str,
        block_index: i64,
        block_type: &str,
        text: &str,
        heading: &str,
        level: Option<i64>,
        page: Option<i64>,
        language: &str,
        markdown: &str,
        html: &str,
        raw_asset_path: &str,
        alt_text: &str,
        caption: &str,
        ocr_text: &str,
    ) -> PreviewBlockView {
        let asset_path = Self::resolve_preview_asset_path(raw_asset_path, document_path);
        if block_type == "image" {
            Self::log_preview_image_block(document_path, raw_asset_path, &asset_path);
        }
        PreviewBlockView {
            block_index: block_index as usize,
            block_type: block_type.to_string(),
            text: text.to_string(),
            heading: heading.to_string(),
            level: level.map(|value| value as u32),
            page: page.map(|value| value as u32),
            language: language.to_string(),
            markdown: markdown.to_string(),
            html: html.to_string(),
            asset_path,
            alt_text: alt_text.to_string(),
            caption: caption.to_string(),
            ocr_text: ocr_text.to_string(),
        }
    }

    pub async fn list_document_chunks(&self, path: &str) -> Result<Vec<ChunkView>, sqlx::Error> {
        let document_id = self.document_id_by_path(path).await?;
        let document_id = match document_id {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        let rows = sqlx::query_as::<_, ChunkRow>(
            r#"
            SELECT
                c.id,
                c.heading,
                c.snippet,
                c.paragraph,
                c.page,
                c.block_indexes_json
            FROM chunks c
            WHERE c.document_id = ?
            ORDER BY c.rowid
            "#,
        )
        .bind(&document_id)
        .fetch_all(&self.pool)
        .await?;

        let block_rows = sqlx::query_as::<_, BlockRow>(
            r#"
            SELECT block_index, block_type, text, heading, level, page, language, markdown, html, asset_path, alt_text, caption, ocr_text
            FROM document_blocks
            WHERE document_id = ?
            ORDER BY block_index
            "#,
        )
        .bind(&document_id)
        .fetch_all(&self.pool)
        .await?;

        let blocks_by_index: std::collections::HashMap<i64, &BlockRow> = block_rows
            .iter()
            .map(|row| (row.block_index, row))
            .collect();

        Ok(rows
            .into_iter()
            .map(|row| {
                let block_indexes: Vec<usize> =
                    serde_json::from_str(&row.block_indexes_json).unwrap_or_default();
                let preview_blocks: Vec<PreviewBlockView> = block_indexes
                    .iter()
                    .filter_map(|index| {
                        blocks_by_index.get(&(*index as i64)).map(|b| {
                            Self::build_preview_block(
                                path,
                                b.block_index,
                                &b.block_type,
                                &b.text,
                                &b.heading,
                                b.level,
                                b.page,
                                &b.language,
                                &b.markdown,
                                &b.html,
                                &b.asset_path,
                                &b.alt_text,
                                &b.caption,
                                &b.ocr_text,
                            )
                        })
                    })
                    .collect();

                ChunkView {
                    id: row.id,
                    heading: row.heading.clone(),
                    title_path: row.heading,
                    snippet: row.snippet,
                    paragraph: row.paragraph.map(|value| value as u32),
                    page: row.page.map(|value| value as u32),
                    preview_blocks,
                }
            })
            .collect())
    }

    pub async fn get_index_status(&self) -> Result<IndexStatusView, sqlx::Error> {
        let indexed_docs = self.count_documents().await?;
        let indexed_chunks = self.count_chunks().await?;
        let pdf_ocr_tasks = self.count_pdf_ocr_tasks().await?;
        let failed_items = self.failed_items().await?;
        let current_task = self.current_task().await?;
        let last_run = self.last_index_run_summary().await?;
        let scanned_docs = current_task
            .as_ref()
            .map(|task| task.total)
            .unwrap_or(indexed_docs as usize);

        Ok(IndexStatusView {
            indexed_docs: indexed_docs as usize,
            indexed_chunks: indexed_chunks as usize,
            pdf_ocr_tasks: pdf_ocr_tasks as usize,
            scanned_docs,
            failed_files: failed_items.len(),
            current_task,
            failed_items,
            last_run,
        })
    }

    pub async fn debug_counts(&self) -> Result<(usize, usize), sqlx::Error> {
        let documents = self.count_documents().await? as usize;
        let chunks = self.count_chunks().await? as usize;
        Ok((documents, chunks))
    }

    pub async fn add_index_dir(&self, path: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_dirs (path, enabled, docs, chunks, status)
            VALUES (?, 1, 0, 0, 'pending')
            "#,
        )
        .bind(path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn remove_index_dir(&self, path: &str) -> Result<(), sqlx::Error> {
        self.clear_directory_documents(path).await?;
        let _ = self.clear_directory_failed_files(path).await;
        sqlx::query("DELETE FROM index_dirs WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_index_dir_enabled(
        &self,
        path: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE index_dirs
            SET enabled = ?
            WHERE path = ?
            "#,
        )
        .bind(enabled as i64)
        .bind(path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn clear_directory_failed_files(
        &self,
        dir_path: &str,
    ) -> Result<(), sqlx::Error> {
        if is_virtual_directory(dir_path) {
            sqlx::query(
                r#"
                DELETE FROM failed_files
                WHERE file = ?
                "#,
            )
            .bind(dir_path)
            .execute(&self.pool)
            .await?;
        } else {
            let prefix = format!("{dir_path}/%");
            sqlx::query(
                r#"
                DELETE FROM failed_files
                WHERE file = ? OR file LIKE ?
                "#,
            )
            .bind(dir_path)
            .bind(prefix)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn record_failed_file(
        &self,
        file: &str,
        reason: &str,
        category: &str,
        code: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT INTO failed_files
                (file, reason, category, code, retry_count, first_failed_at, last_failed_at)
            VALUES (?, ?, ?, ?, 1, ?, ?)
            ON CONFLICT(file) DO UPDATE SET
                reason = excluded.reason,
                category = excluded.category,
                code = excluded.code,
                retry_count = failed_files.retry_count + 1,
                last_failed_at = excluded.last_failed_at
            "#,
        )
        .bind(file)
        .bind(reason)
        .bind(category)
        .bind(code)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn enabled_index_dir_paths(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT path
            FROM index_dirs
            WHERE enabled = 1
            ORDER BY path
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect())
    }

    pub async fn clear_directory_documents(
        &self,
        dir_path: &str,
    ) -> Result<(), sqlx::Error> {
        self.search_index
            .delete_directory(dir_path)
            .map_err(sqlx::Error::Protocol)?;
        if is_virtual_directory(dir_path) {
            // 修复：虚拟目录删除时要同步清掉挂在文档上的 OCR 任务，避免任务表残留旧记录。
            sqlx::query(
                r#"
                DELETE FROM pdf_ocr_tasks
                WHERE document_id IN (
                    SELECT id
                    FROM documents
                    WHERE dir_path = ?
                )
                "#,
            )
            .bind(dir_path)
            .execute(&self.pool)
            .await?;
            sqlx::query(
                r#"
                DELETE FROM chunk_embeddings
                WHERE document_id IN (
                    SELECT id
                    FROM documents
                    WHERE dir_path = ?
                )
                "#,
            )
            .bind(dir_path)
            .execute(&self.pool)
            .await?;
            sqlx::query(
                r#"
                DELETE FROM documents
                WHERE dir_path = ?
                "#,
            )
            .bind(dir_path)
            .execute(&self.pool)
            .await?;
        } else {
            let prefix = format!("{dir_path}/%");
            // 修复：按目录批量删除时，OCR 任务表也必须按文件路径前缀一并清理。
            sqlx::query(
                r#"
                DELETE FROM pdf_ocr_tasks
                WHERE document_path = ? OR document_path LIKE ?
                "#,
            )
            .bind(dir_path)
            .bind(prefix.clone())
            .execute(&self.pool)
            .await?;
            sqlx::query(
                r#"
                DELETE FROM chunk_embeddings
                WHERE document_id IN (
                    SELECT id
                    FROM documents
                    WHERE path = ? OR path LIKE ?
                )
                "#,
            )
            .bind(dir_path)
            .bind(prefix.clone())
            .execute(&self.pool)
            .await?;
            sqlx::query(
                r#"
                DELETE FROM documents
                WHERE path = ? OR path LIKE ?
                "#,
            )
            .bind(dir_path)
            .bind(prefix)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn clear_document_by_path(&self, path: &str) -> Result<(), sqlx::Error> {
        self.search_index
            .delete_document(path)
            .map_err(sqlx::Error::Protocol)?;
        // 修复：单文件重跑时必须清掉旧 OCR 任务，否则新一轮解析会把同一路径的任务重复累计。
        sqlx::query(
            r#"
            DELETE FROM pdf_ocr_tasks
            WHERE document_id IN (
                SELECT id
                FROM documents
                WHERE path = ?
            )
            "#,
        )
        .bind(path)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"
            DELETE FROM chunk_embeddings
            WHERE document_id IN (
                SELECT id
                FROM documents
                WHERE path = ?
            )
            "#,
        )
        .bind(path)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"
            DELETE FROM documents
            WHERE path = ?
            "#,
        )
        .bind(path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn document_id_by_path(
        &self,
        path: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            r#"
            SELECT id
            FROM documents
            WHERE path = ?
            LIMIT 1
            "#,
        )
        .bind(path)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn store_document(
        &self,
        document: &ExtractedDocument,
        chunks: &[ChunkRecord],
        blocks: &[crate::seekmind::parser::types::ParsedBlock],
        ocr_tasks: &[PdfOcrTask],
    ) -> Result<(), sqlx::Error> {
        self.clear_document_by_path(&document.path).await?;
        let document_id = Uuid::new_v4().to_string();
        eprintln!(
            "[SeekMind] store_document path={} chunks={} blocks={} ocr_tasks={}",
            document.path,
            chunks.len(),
            blocks.len(),
            ocr_tasks.len()
        );

        sqlx::query(
            r#"
            INSERT INTO documents
                (id, dir_path, path, file_name, ext, file_size, modified_at, content_hash, modified, content)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&document_id)
        .bind(&document.dir_path)
        .bind(&document.path)
        .bind(&document.file_name)
        .bind(&document.ext)
        .bind(document.file_size)
        .bind(document.modified_at)
        .bind(&document.content_hash)
        .bind(&document.modified)
        .bind(&document.content)
        .execute(&self.pool)
        .await?;

        for block in blocks {
            let block_id = format!("{document_id}:{}", block.block_index);
            sqlx::query(
                r#"
                INSERT INTO document_blocks
                    (id, document_id, block_index, block_type, text, heading, level, page, language, markdown, html, asset_path, alt_text, caption, ocr_text)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&block_id)
            .bind(&document_id)
            .bind(block.block_index as i64)
            .bind(&block.block_type)
            .bind(&block.text)
            .bind(block.heading.as_deref().unwrap_or(""))
            .bind(block.level.map(|v| v as i64))
            .bind(block.page_no.map(|v| v as i64))
            .bind(block.language.as_deref().unwrap_or(""))
            .bind(block.markdown.as_deref().unwrap_or(""))
            .bind(block.html.as_deref().unwrap_or(""))
            .bind(block.asset_path.as_deref().unwrap_or(""))
            .bind(block.alt_text.as_deref().unwrap_or(""))
            .bind(block.caption.as_deref().unwrap_or(""))
            .bind(block.ocr_text.as_deref().unwrap_or(""))
            .execute(&self.pool)
            .await?;
        }

        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_id = format!("{document_id}:{index}");
            let block_indexes_json =
                serde_json::to_string(&chunk.block_indexes).unwrap_or_else(|_| "[]".to_string());
            sqlx::query(
                r#"
                INSERT INTO chunks (id, document_id, heading, snippet, paragraph, page, score, block_indexes_json)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(chunk_id)
            .bind(&document_id)
            .bind(&chunk.heading)
            .bind(&chunk.snippet)
            .bind(chunk.paragraph)
            .bind(chunk.page)
            .bind(chunk.score)
            .bind(&block_indexes_json)
            .execute(&self.pool)
            .await?;
        }

        self.store_pdf_ocr_tasks(&document_id, &document.path, ocr_tasks)
            .await?;

        if let Err(error) = self
            .search_index
            .index_document(&document_id, document, chunks)
        {
            sqlx::query(
                r#"
                DELETE FROM documents
                WHERE path = ?
                "#,
            )
            .bind(&document.path)
            .execute(&self.pool)
            .await?;

            return Err(sqlx::Error::Protocol(error.into()));
        }

        // Keep parsing/indexing responsive. Semantic vectors are rebuilt by the
        // dedicated semantic job instead of blocking document refresh.
        sqlx::query(
            r#"
            UPDATE vector_index_meta
            SET status = 'needs_rebuild',
                last_error = ''
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn replace_failed_files(
        &self,
        failed_items: &[(String, String)],
    ) -> Result<(), sqlx::Error> {
        self.clear_failed_files().await?;

        for (file, reason) in failed_items {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO failed_files (file, reason)
                VALUES (?, ?)
                "#,
            )
            .bind(file)
            .bind(reason)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn clear_failed_files(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM failed_files")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn clear_failed_file(&self, path: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM failed_files WHERE file = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn store_pdf_ocr_tasks(
        &self,
        document_id: &str,
        document_path: &str,
        ocr_tasks: &[PdfOcrTask],
    ) -> Result<(), sqlx::Error> {
        if ocr_tasks.is_empty() {
            return Ok(());
        }

        for task in ocr_tasks {
            let task_id = format!("{document_id}:ocr:{}", task.page_index);
            sqlx::query(
                r#"
                INSERT INTO pdf_ocr_tasks
                    (id, document_id, document_path, page_index, reason, message, warning, status, ocr_text, error, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(task_id)
            .bind(document_id)
            .bind(document_path)
            .bind(task.page_index as i64)
            .bind(task.reason.trim())
            .bind(task.message.trim())
            .bind(task.warning.as_deref().unwrap_or(""))
            .bind(if task.status.trim().is_empty() {
                "queued"
            } else {
                task.status.trim()
            })
            .bind(task.ocr_text.as_deref().unwrap_or(""))
            .bind(task.error.as_deref().unwrap_or(""))
            .bind(current_unix_ts())
            .bind(current_unix_ts())
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn count_documents(&self) -> Result<i64, sqlx::Error> {
        scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM documents").await
    }

    async fn count_chunks(&self) -> Result<i64, sqlx::Error> {
        scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM chunks").await
    }

    async fn count_pdf_ocr_tasks(&self) -> Result<i64, sqlx::Error> {
        scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM pdf_ocr_tasks").await
    }

    pub(crate) async fn pending_pdf_ocr_document_paths(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT document_path
            FROM pdf_ocr_tasks
            WHERE TRIM(document_path) <> ''
              AND status IN ('queued', 'failed', 'skipped')
            GROUP BY document_path
            ORDER BY MAX(updated_at) ASC, document_path ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect())
    }
}
