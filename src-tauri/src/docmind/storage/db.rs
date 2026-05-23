#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

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
use crate::docmind::storage::types::{ChunkRecord, ExtractedDocument};

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
}

#[derive(Debug, sqlx::FromRow)]
struct CurrentTaskRow {
    label: String,
    details: String,
    current_dir: String,
    current_file: String,
    progress: i64,
    scanned: i64,
    total: i64,
    succeeded: i64,
    failed: i64,
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
            .max_connections(1)
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
        let document_id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO documents (id, dir_path, path, file_name, ext, modified, content)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&document_id)
        .bind(&document.dir_path)
        .bind(&document.path)
        .bind(&document.file_name)
        .bind(&document.ext)
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

    pub(crate) async fn set_current_task(
        &self,
        label: &str,
        details: &str,
        current_dir: &str,
        current_file: &str,
        progress: u8,
        scanned: usize,
        total: usize,
        succeeded: usize,
        failed: usize,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO current_task
                (id, label, details, current_dir, current_file, progress, scanned, total, succeeded, failed)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(label)
        .bind(details)
        .bind(current_dir)
        .bind(current_file)
        .bind(progress as i64)
        .bind(scanned as i64)
        .bind(total as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
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
                reason TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS current_task (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                label TEXT NOT NULL,
                details TEXT NOT NULL,
                current_dir TEXT NOT NULL DEFAULT '',
                current_file TEXT NOT NULL DEFAULT '',
                progress INTEGER NOT NULL,
                scanned INTEGER NOT NULL,
                total INTEGER NOT NULL,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0
            )
            "#,
        ];

        for statement in statements {
            sqlx::query(statement).execute(&self.pool).await?;
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

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

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
            SELECT file, reason
            FROM failed_files
            ORDER BY file
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| FailedFileView {
                file: row.file,
                reason: row.reason,
            })
            .collect())
    }

    async fn current_task(&self) -> Result<Option<CurrentTaskView>, sqlx::Error> {
        let row = sqlx::query_as::<_, CurrentTaskRow>(
            r#"
            SELECT label, details, current_dir, current_file, progress, scanned, total, succeeded, failed
            FROM current_task
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| CurrentTaskView {
            label: row.label,
            details: row.details,
            current_dir: row.current_dir,
            current_file: row.current_file,
            progress: row.progress as u8,
            scanned: row.scanned as usize,
            total: row.total as usize,
            succeeded: row.succeeded as usize,
            failed: row.failed as usize,
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
