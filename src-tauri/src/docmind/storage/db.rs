#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use dirs::{data_dir, download_dir, document_dir};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::docmind::file_ops;
use crate::docmind::models::{
    ChunkView, CurrentTaskView, DocumentView, FailedFileView, IndexDirView, IndexStatusView,
    SearchResultView,
};
use crate::docmind::storage::fulltext::SearchIndex;
use crate::docmind::storage::indexer;
use crate::docmind::storage::types::{ChunkRecord, DocumentState, ExtractedDocument, IndexSettings};

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
    search_index: Arc<SearchIndex>,
}

#[derive(Debug, sqlx::FromRow)]
struct IndexDirRow {
    path: String,
    enabled: i64,
    docs: i64,
    chunks: i64,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchRow {
    id: String,
    file_name: String,
    path: String,
    ext: String,
    heading: String,
    snippet: String,
    paragraph: Option<i64>,
    page: Option<i64>,
    modified: String,
    score: f32,
}

#[derive(Debug, sqlx::FromRow)]
struct DocumentRow {
    id: String,
    dir_path: String,
    path: String,
    file_name: String,
    ext: String,
    modified: String,
    chunks: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ChunkRow {
    id: String,
    heading: String,
    snippet: String,
    paragraph: Option<i64>,
    page: Option<i64>,
}

#[derive(Debug, sqlx::FromRow)]
struct FailedFileRow {
    file: String,
    reason: String,
    category: String,
    code: String,
    retry_count: i64,
    last_failed_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct IndexRunSummaryRow {
    updated: i64,
    skipped: i64,
    deleted: i64,
    scanned: i64,
    total: i64,
    succeeded: i64,
    failed: i64,
    completed_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct IndexCheckpointRow {
    pub(crate) dir_paths: String,
    pub(crate) pending_delete_paths: String,
    pub(crate) pending_update_paths: String,
    pub(crate) phase: String,
    pub(crate) current_dir: String,
    pub(crate) current_file: String,
    pub(crate) total: i64,
    pub(crate) processed: i64,
    pub(crate) succeeded: i64,
    pub(crate) failed: i64,
    pub(crate) updated: i64,
    pub(crate) skipped: i64,
    pub(crate) deleted: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct CurrentTaskRow {
    label: String,
    details: String,
    state: String,
    current_dir: String,
    current_file: String,
    progress: i64,
    scanned: i64,
    total: i64,
    succeeded: i64,
    failed: i64,
    updated: i64,
    skipped: i64,
    deleted: i64,
    pause_requested: i64,
}

impl Database {
    pub async fn open_or_init() -> Result<Self, String> {
        let path = database_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .map_err(|error| error.to_string())?;

        let search_index = Arc::new(SearchIndex::open_or_init()?);
        let database = Self { pool, search_index };
        database
            .init_schema()
            .await
            .map_err(|error| error.to_string())?;
        let documents_migrated = database
            .ensure_documents_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_current_task_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_failed_files_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_index_settings_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_index_run_summary_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_index_checkpoint_table()
            .await
            .map_err(|error| error.to_string())?;

        if documents_migrated {
            database
                .clear_all_index_data()
                .await
                .map_err(|error| error.to_string())?;
        }

        database
            .seed_default_dirs_if_empty()
            .await
            .map_err(|error| error.to_string())?;

        let skip_bootstrap_index = std::env::var("DOCMIND_SKIP_BOOTSTRAP_INDEX").is_ok();
        if !skip_bootstrap_index {
            let indexed_docs = database
                .count_documents()
                .await
                .map_err(|error| error.to_string())?;

            if indexed_docs == 0 {
                let _ = indexer::rebuild_all(&database).await;
            } else if database.search_index.doc_count() == 0 {
                let _ = indexer::rebuild_all(&database).await;
            }
        }

        Ok(database)
    }

    pub async fn get_index_settings(&self) -> Result<IndexSettings, sqlx::Error> {
        #[derive(Debug, sqlx::FromRow)]
        struct IndexSettingsRow {
            exclude_dirs: String,
            exclude_exts: String,
            max_file_size_mb: i64,
        }

        let row = sqlx::query_as::<_, IndexSettingsRow>(
            r#"
            SELECT exclude_dirs, exclude_exts, max_file_size_mb
            FROM index_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(IndexSettings {
                exclude_dirs: serde_json::from_str(&row.exclude_dirs).unwrap_or_default(),
                exclude_exts: serde_json::from_str(&row.exclude_exts).unwrap_or_default(),
                max_file_size_mb: row.max_file_size_mb.max(0) as u64,
            })
        } else {
            Ok(IndexSettings {
                exclude_dirs: default_exclude_dirs(),
                exclude_exts: Vec::new(),
                max_file_size_mb: 50,
            })
        }
    }

    pub async fn save_index_settings(&self, settings: &IndexSettings) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_settings
                (id, exclude_dirs, exclude_exts, max_file_size_mb)
            VALUES (
                1,
                ?,
                ?,
                ?
            )
            "#,
        )
        .bind(serde_json::to_string(&settings.exclude_dirs).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&settings.exclude_exts).unwrap_or_else(|_| "[]".to_string()))
        .bind(settings.max_file_size_mb as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_index_dirs(&self) -> Result<Vec<IndexDirView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, IndexDirRow>(
            r#"
            SELECT path, enabled, docs, chunks, status
            FROM index_dirs
            ORDER BY path
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| IndexDirView {
                path: row.path,
                enabled: row.enabled != 0,
                docs: row.docs as usize,
                chunks: row.chunks as usize,
                status: row.status,
            })
            .collect())
    }

    pub async fn search_documents(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResultView>, sqlx::Error> {
        let hits = self
            .search_index
            .search(query, limit.max(1))
            .map_err(sqlx::Error::Protocol)?;

        if hits.is_empty() {
            return Ok(Vec::new());
        }

        let chunk_ids = hits.iter().map(|hit| hit.chunk_id.clone()).collect::<Vec<_>>();
        let rows = self.fetch_chunks_by_ids(&chunk_ids).await?;
        let mut rows_by_id = std::collections::HashMap::new();
        for row in rows {
            rows_by_id.insert(row.id.clone(), row);
        }

        let mut results = Vec::new();
        for hit in hits {
            if let Some(row) = rows_by_id.remove(&hit.chunk_id) {
                results.push(SearchResultView {
                    id: row.id,
                    file_name: row.file_name,
                    path: row.path,
                    ext: row.ext,
                    heading: row.heading,
                    snippet: row.snippet,
                    paragraph: row.paragraph.map(|value| value as u32),
                    page: row.page.map(|value| value as u32),
                    modified: row.modified,
                    score: hit.score,
                });
            }
        }

        Ok(results)
    }

    pub async fn list_documents_in_dir(
        &self,
        dir_path: &str,
    ) -> Result<Vec<DocumentView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, DocumentRow>(
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
        .await?;

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

        let rows = sqlx::query_as::<_, DocumentStateRow>(
            r#"
            SELECT path, file_size, modified_at, content_hash
            FROM documents
            WHERE dir_path = ?
            "#,
        )
        .bind(dir_path)
        .fetch_all(&self.pool)
        .await?;

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

    pub async fn list_document_chunks(&self, path: &str) -> Result<Vec<ChunkView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ChunkRow>(
            r#"
            SELECT
                c.id,
                c.heading,
                c.snippet,
                c.paragraph,
                c.page
            FROM chunks c
            INNER JOIN documents d ON d.id = c.document_id
            WHERE d.path = ?
            ORDER BY c.rowid
            "#,
        )
        .bind(path)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ChunkView {
                id: row.id,
                heading: row.heading,
                snippet: row.snippet,
                paragraph: row.paragraph.map(|value| value as u32),
                page: row.page.map(|value| value as u32),
            })
            .collect())
    }

    pub async fn get_index_status(&self) -> Result<IndexStatusView, sqlx::Error> {
        let indexed_docs = self.count_documents().await?;
        let indexed_chunks = self.count_chunks().await?;
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

    pub fn tantivy_document_count(&self) -> usize {
        self.search_index.doc_count() as usize
    }

    pub async fn open_file(&self, path: &str) -> Result<(), String> {
        let _ = &self.pool;
        file_ops::open_file_path(path)
    }

    pub(crate) async fn add_index_dir(&self, path: &str) -> Result<(), sqlx::Error> {
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

    pub(crate) async fn remove_index_dir(&self, path: &str) -> Result<(), sqlx::Error> {
        self.search_index
            .delete_directory(path)
            .map_err(sqlx::Error::Protocol)?;
        sqlx::query("DELETE FROM index_dirs WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn set_index_dir_enabled(
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

    pub(crate) async fn clear_directory_failed_files(
        &self,
        dir_path: &str,
    ) -> Result<(), sqlx::Error> {
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
        Ok(())
    }

    pub(crate) async fn record_failed_file(
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

    pub(crate) async fn enabled_index_dir_paths(&self) -> Result<Vec<String>, sqlx::Error> {
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

    pub(crate) async fn clear_directory_documents(&self, dir_path: &str) -> Result<(), sqlx::Error> {
        self.search_index
            .delete_directory(dir_path)
            .map_err(sqlx::Error::Protocol)?;
        sqlx::query(
            r#"
            DELETE FROM documents
            WHERE dir_path = ?
            "#,
        )
        .bind(dir_path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn clear_document_by_path(&self, path: &str) -> Result<(), sqlx::Error> {
        self.search_index
            .delete_document(path)
            .map_err(sqlx::Error::Protocol)?;
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

    pub(crate) async fn store_document(
        &self,
        document: &ExtractedDocument,
        chunks: &[ChunkRecord],
    ) -> Result<(), sqlx::Error> {
        self.clear_document_by_path(&document.path).await?;
        let document_id = Uuid::new_v4().to_string();

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

        for (index, chunk) in chunks.iter().enumerate() {
            let chunk_id = format!("{document_id}:{index}");
            sqlx::query(
                r#"
                INSERT INTO chunks (id, document_id, heading, snippet, paragraph, page, score)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(chunk_id)
            .bind(&document_id)
            .bind(&chunk.heading)
            .bind(&chunk.snippet)
            .bind(chunk.paragraph)
            .bind(chunk.page)
            .bind(chunk.score)
            .execute(&self.pool)
            .await?;
        }

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

        Ok(())
    }

    pub(crate) async fn replace_failed_files(
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

    pub(crate) async fn clear_failed_files(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM failed_files")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn clear_failed_file(&self, path: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM failed_files WHERE file = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn save_index_run_summary(
        &self,
        updated: usize,
        skipped: usize,
        deleted: usize,
        scanned: usize,
        total: usize,
        succeeded: usize,
        failed: usize,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_run_summary
                (id, updated, skipped, deleted, scanned, total, succeeded, failed, completed_at)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(updated as i64)
        .bind(skipped as i64)
        .bind(deleted as i64)
        .bind(scanned as i64)
        .bind(total as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn last_index_run_summary(&self) -> Result<Option<crate::docmind::models::IndexRunSummaryView>, sqlx::Error> {
        let row = sqlx::query_as::<_, IndexRunSummaryRow>(
            r#"
            SELECT updated, skipped, deleted, scanned, total, succeeded, failed, completed_at
            FROM index_run_summary
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| crate::docmind::models::IndexRunSummaryView {
            updated: row.updated as usize,
            skipped: row.skipped as usize,
            deleted: row.deleted as usize,
            scanned: row.scanned as usize,
            total: row.total as usize,
            succeeded: row.succeeded as usize,
            failed: row.failed as usize,
            completed_at: format_unix_ts(row.completed_at),
        }))
    }

    pub(crate) async fn save_index_checkpoint(
        &self,
        dir_paths: &[String],
        pending_delete_paths: &[String],
        pending_update_paths: &[String],
        phase: &str,
        current_dir: &str,
        current_file: &str,
        total: usize,
        processed: usize,
        succeeded: usize,
        failed: usize,
        updated: usize,
        skipped: usize,
        deleted: usize,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_checkpoint
                (id, dir_paths, pending_delete_paths, pending_update_paths, phase, current_dir, current_file, total, processed, succeeded, failed, updated, skipped, deleted)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(serde_json::to_string(dir_paths).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(pending_delete_paths).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(pending_update_paths).unwrap_or_else(|_| "[]".to_string()))
        .bind(phase)
        .bind(current_dir)
        .bind(current_file)
        .bind(total as i64)
        .bind(processed as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
        .bind(updated as i64)
        .bind(skipped as i64)
        .bind(deleted as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn load_index_checkpoint(&self) -> Result<Option<IndexCheckpointRow>, sqlx::Error> {
        sqlx::query_as::<_, IndexCheckpointRow>(
            r#"
            SELECT dir_paths, pending_delete_paths, pending_update_paths, phase, current_dir, current_file, total, processed, succeeded, failed, updated, skipped, deleted
            FROM index_checkpoint
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub(crate) async fn clear_index_checkpoint(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM index_checkpoint WHERE id = 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn set_current_task(
        &self,
        label: &str,
        details: &str,
        state: &str,
        current_dir: &str,
        current_file: &str,
        progress: u8,
        scanned: usize,
        total: usize,
        succeeded: usize,
        failed: usize,
        updated: usize,
        skipped: usize,
        deleted: usize,
        pause_requested: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO current_task
                (id, label, details, state, current_dir, current_file, progress, scanned, total, succeeded, failed, updated, skipped, deleted, pause_requested)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(label)
        .bind(details)
        .bind(state)
        .bind(current_dir)
        .bind(current_file)
        .bind(progress as i64)
        .bind(scanned as i64)
        .bind(total as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
        .bind(updated as i64)
        .bind(skipped as i64)
        .bind(deleted as i64)
        .bind(pause_requested as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn clear_current_task(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM current_task WHERE id = 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn request_pause_current_task(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE current_task
            SET pause_requested = 1
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn clear_pause_request(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE current_task
            SET pause_requested = 0
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn current_task_pause_requested(&self) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT pause_requested FROM current_task WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;

        Ok(row
            .and_then(|row| row.try_get::<i64, _>(0).ok())
            .map(|value| value != 0)
            .unwrap_or(false))
    }

    pub(crate) async fn clear_all_index_data(&self) -> Result<(), sqlx::Error> {
        self.search_index
            .clear_all()
            .map_err(sqlx::Error::Protocol)?;

        sqlx::query("DELETE FROM chunks")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM documents")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM failed_files")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM current_task")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM index_run_summary")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"
            UPDATE index_dirs
            SET docs = 0,
                chunks = 0,
                status = 'pending'
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn set_index_dir_status(
        &self,
        path: &str,
        docs: usize,
        chunks: usize,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE index_dirs
            SET docs = ?, chunks = ?, status = ?
            WHERE path = ?
            "#,
        )
        .bind(docs as i64)
        .bind(chunks as i64)
        .bind(status)
        .bind(path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn refresh_index_dir_stats(&self, path: &str) -> Result<(), sqlx::Error> {
        let docs = scalar_count_bind(
            &self.pool,
            r#"
            SELECT COUNT(*)
            FROM documents
            WHERE dir_path = ?
            "#,
            path,
        )
        .await? as usize;

        let chunks = scalar_count_bind(
            &self.pool,
            r#"
            SELECT COUNT(*)
            FROM chunks c
            INNER JOIN documents d ON d.id = c.document_id
            WHERE d.dir_path = ?
            "#,
            path,
        )
        .await? as usize;

        let status = if docs == 0 { "empty" } else { "indexed" };
        self.set_index_dir_status(path, docs, chunks, status).await
    }

    async fn init_schema(&self) -> Result<(), sqlx::Error> {
        let statements = [
            r#"
            CREATE TABLE IF NOT EXISTS index_dirs (
                path TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                docs INTEGER NOT NULL DEFAULT 0,
                chunks INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                dir_path TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                file_name TEXT NOT NULL,
                ext TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                modified_at INTEGER NOT NULL DEFAULT 0,
                content_hash TEXT NOT NULL DEFAULT '',
                modified TEXT NOT NULL,
                content TEXT NOT NULL,
                FOREIGN KEY(dir_path) REFERENCES index_dirs(path) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                heading TEXT NOT NULL,
                snippet TEXT NOT NULL,
                paragraph INTEGER,
                page INTEGER,
                score REAL NOT NULL,
                FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS failed_files (
                file TEXT PRIMARY KEY,
                reason TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'unknown',
                code TEXT NOT NULL DEFAULT 'unknown',
                retry_count INTEGER NOT NULL DEFAULT 0,
                first_failed_at INTEGER NOT NULL DEFAULT 0,
                last_failed_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS current_task (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                label TEXT NOT NULL,
                details TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'idle',
                current_dir TEXT NOT NULL DEFAULT '',
                current_file TEXT NOT NULL DEFAULT '',
                progress INTEGER NOT NULL,
                scanned INTEGER NOT NULL,
                total INTEGER NOT NULL,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0,
                pause_requested INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS index_run_summary (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0,
                scanned INTEGER NOT NULL DEFAULT 0,
                total INTEGER NOT NULL DEFAULT 0,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                completed_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS index_checkpoint (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                dir_paths TEXT NOT NULL,
                pending_delete_paths TEXT NOT NULL,
                pending_update_paths TEXT NOT NULL,
                phase TEXT NOT NULL,
                current_dir TEXT NOT NULL DEFAULT '',
                current_file TEXT NOT NULL DEFAULT '',
                total INTEGER NOT NULL DEFAULT 0,
                processed INTEGER NOT NULL DEFAULT 0,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS index_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                exclude_dirs TEXT NOT NULL,
                exclude_exts TEXT NOT NULL,
                max_file_size_mb INTEGER NOT NULL
            )
            "#,
        ];

        for statement in statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    async fn ensure_index_settings_row(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM index_settings").await?;
        if count == 0 {
            let defaults = IndexSettings {
                exclude_dirs: default_exclude_dirs(),
                exclude_exts: Vec::new(),
                max_file_size_mb: 50,
            };
            self.save_index_settings(&defaults).await?;
        }
        Ok(())
    }

    async fn ensure_current_task_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(current_task)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("current_dir") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN current_dir TEXT NOT NULL DEFAULT ''",
            );
        }
        if !columns.contains("current_file") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN current_file TEXT NOT NULL DEFAULT ''",
            );
        }
        if !columns.contains("succeeded") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN succeeded INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("failed") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN failed INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("updated") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN updated INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("skipped") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN skipped INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("deleted") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("state") {
            alter_statements.push("ALTER TABLE current_task ADD COLUMN state TEXT NOT NULL DEFAULT 'idle'");
        }
        if !columns.contains("pause_requested") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN pause_requested INTEGER NOT NULL DEFAULT 0",
            );
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    async fn ensure_failed_files_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(failed_files)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("category") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN category TEXT NOT NULL DEFAULT 'unknown'",
            );
        }
        if !columns.contains("code") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN code TEXT NOT NULL DEFAULT 'unknown'",
            );
        }
        if !columns.contains("retry_count") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("first_failed_at") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN first_failed_at INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("last_failed_at") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN last_failed_at INTEGER NOT NULL DEFAULT 0",
            );
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    async fn ensure_index_run_summary_row(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM index_run_summary").await?;
        if count == 0 {
            self.save_index_run_summary(0, 0, 0, 0, 0, 0, 0).await?;
        }
        Ok(())
    }

    async fn ensure_index_checkpoint_table(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS index_checkpoint (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                dir_paths TEXT NOT NULL,
                pending_delete_paths TEXT NOT NULL,
                pending_update_paths TEXT NOT NULL,
                phase TEXT NOT NULL,
                current_dir TEXT NOT NULL DEFAULT '',
                current_file TEXT NOT NULL DEFAULT '',
                total INTEGER NOT NULL DEFAULT 0,
                processed INTEGER NOT NULL DEFAULT 0,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn ensure_documents_columns(&self) -> Result<bool, sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(documents)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut altered = false;
        let mut alter_statements = Vec::new();
        if !columns.contains("dir_path") {
            alter_statements.push(
                "ALTER TABLE documents ADD COLUMN dir_path TEXT NOT NULL DEFAULT ''",
            );
            altered = true;
        }
        if !columns.contains("file_size") {
            alter_statements.push(
                "ALTER TABLE documents ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0",
            );
            altered = true;
        }
        if !columns.contains("modified_at") {
            alter_statements.push(
                "ALTER TABLE documents ADD COLUMN modified_at INTEGER NOT NULL DEFAULT 0",
            );
            altered = true;
        }
        if !columns.contains("content_hash") {
            alter_statements.push(
                "ALTER TABLE documents ADD COLUMN content_hash TEXT NOT NULL DEFAULT ''",
            );
            altered = true;
        }
        if !columns.contains("content") {
            alter_statements.push(
                "ALTER TABLE documents ADD COLUMN content TEXT NOT NULL DEFAULT ''",
            );
            altered = true;
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(altered)
    }

    async fn seed_default_dirs_if_empty(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM index_dirs").await?;
        if count > 0 {
            return Ok(());
        }

        let mut default_dirs = Vec::new();
        if let Some(path) = document_dir() {
            default_dirs.push(path);
        }
        if let Some(path) = download_dir() {
            default_dirs.push(path);
        }

        for dir in default_dirs {
            let path = dir.to_string_lossy().to_string();
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO index_dirs (path, enabled, docs, chunks, status)
                VALUES (?, 1, 0, 0, 'pending')
                "#,
            )
            .bind(path)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn failed_items(&self) -> Result<Vec<FailedFileView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, FailedFileRow>(
            r#"
            SELECT file, reason, category, code, retry_count, last_failed_at
            FROM failed_files
            ORDER BY category, file
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| FailedFileView {
                file: row.file,
                reason: row.reason,
                category: row.category,
                code: row.code,
                retry_count: row.retry_count.max(0) as usize,
                last_failed_at: format_unix_ts(row.last_failed_at),
            })
            .collect())
    }

    async fn current_task(&self) -> Result<Option<CurrentTaskView>, sqlx::Error> {
        let row = sqlx::query_as::<_, CurrentTaskRow>(
            r#"
            SELECT label, details, state, current_dir, current_file, progress, scanned, total, succeeded, failed, updated, skipped, deleted, pause_requested
            FROM current_task
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| CurrentTaskView {
            label: row.label,
            details: row.details,
            state: row.state,
            current_dir: row.current_dir,
            current_file: row.current_file,
            progress: row.progress as u8,
            scanned: row.scanned as usize,
            total: row.total as usize,
            succeeded: row.succeeded as usize,
            failed: row.failed as usize,
            updated: row.updated as usize,
            skipped: row.skipped as usize,
            deleted: row.deleted as usize,
            pause_requested: row.pause_requested != 0,
        }))
    }

    async fn fetch_chunks_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<SearchRow>, sqlx::Error> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = std::iter::repeat("?")
            .take(chunk_ids.len())
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            r#"
            SELECT
                c.id,
                d.file_name,
                d.path,
                d.ext,
                c.heading,
                c.snippet,
                c.paragraph,
                c.page,
                d.modified,
                c.score
            FROM chunks c
            JOIN documents d ON d.id = c.document_id
            WHERE c.id IN ({})
            "#,
            placeholders
        );

        let mut query_builder = sqlx::query_as::<_, SearchRow>(&sql);
        for chunk_id in chunk_ids {
            query_builder = query_builder.bind(chunk_id);
        }

        query_builder.fetch_all(&self.pool).await
    }

    async fn count_documents(&self) -> Result<i64, sqlx::Error> {
        scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM documents").await
    }

    async fn count_chunks(&self) -> Result<i64, sqlx::Error> {
        scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM chunks").await
    }
}

async fn scalar_count_no_bind(pool: &SqlitePool, sql: &str) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(sql).fetch_one(pool).await?;
    row.try_get::<i64, _>(0)
}

async fn scalar_count_bind(pool: &SqlitePool, sql: &str, bind: &str) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(sql).bind(bind).fetch_one(pool).await?;
    row.try_get::<i64, _>(0)
}

fn database_path() -> PathBuf {
    let base = data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("DocMind").join("docmind.sqlite")
}

pub fn sqlite_database_path() -> PathBuf {
    database_path()
}

fn default_exclude_dirs() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        ".git".to_string(),
        "target".to_string(),
        "Library".to_string(),
        "Caches".to_string(),
        "Application Support".to_string(),
    ]
}

fn current_unix_ts() -> i64 {
    Utc::now().timestamp()
}

fn format_unix_ts(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "未知".to_string();
    }

    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "未知".to_string())
}
