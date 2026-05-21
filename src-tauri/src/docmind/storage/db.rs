#![allow(dead_code)]

use std::path::PathBuf;

use dirs::{data_dir, download_dir, document_dir};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::docmind::file_ops;
use crate::docmind::models::{
    CurrentTaskView, FailedFileView, IndexDirView, IndexStatusView, SearchResultView,
};
use crate::docmind::search;
use crate::docmind::storage::indexer;
use crate::docmind::storage::types::{ChunkRecord, ExtractedDocument};

#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
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
struct FailedFileRow {
    file: String,
    reason: String,
}

#[derive(Debug, sqlx::FromRow)]
struct CurrentTaskRow {
    label: String,
    details: String,
    progress: i64,
    scanned: i64,
    total: i64,
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

        let database = Self { pool };
        database
            .init_schema()
            .await
            .map_err(|error| error.to_string())?;
        database
            .seed_default_dirs_if_empty()
            .await
            .map_err(|error| error.to_string())?;

        if database
            .count_documents()
            .await
            .map_err(|error| error.to_string())?
            == 0
        {
            let _ = indexer::rebuild_all(&database).await;
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
        let query_terms = search::normalize_query(query);

        let mut sql = String::from(
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
            "#,
        );

        let mut binds = Vec::new();
        if !query_terms.is_empty() {
            sql.push_str(" WHERE ");
            for (index, term) in query_terms.iter().enumerate() {
                if index > 0 {
                    sql.push_str(" AND ");
                }
                sql.push_str(
                    "(lower(d.file_name) LIKE ? OR lower(d.path) LIKE ? OR lower(d.ext) LIKE ? OR lower(d.content) LIKE ? OR lower(c.heading) LIKE ? OR lower(c.snippet) LIKE ?)",
                );

                let wildcard = format!("%{}%", term.to_lowercase());
                for _ in 0..6 {
                    binds.push(wildcard.clone());
                }
            }
        }

        sql.push_str(" ORDER BY c.score DESC, d.modified DESC LIMIT ?");

        let mut query_builder = sqlx::query_as::<_, SearchRow>(&sql);
        for bind in binds {
            query_builder = query_builder.bind(bind);
        }
        query_builder = query_builder.bind(limit.max(1) as i64);

        let rows = query_builder.fetch_all(&self.pool).await?;
        let candidates = rows
            .into_iter()
            .map(|row| SearchResultView {
                id: row.id,
                file_name: row.file_name,
                path: row.path,
                ext: row.ext,
                heading: row.heading,
                snippet: row.snippet,
                paragraph: row.paragraph.map(|value| value as u32),
                page: row.page.map(|value| value as u32),
                modified: row.modified,
                score: row.score,
            })
            .collect::<Vec<_>>();

        let mut matched = search::filter_results(&query_terms, &candidates);
        if matched.is_empty() {
            matched = search::relax_results(&query_terms, &candidates);
        }

        Ok(search::sort_results(matched)
            .into_iter()
            .take(limit.max(1))
            .collect())
    }

    pub async fn get_index_status(&self) -> Result<IndexStatusView, sqlx::Error> {
        let indexed_docs = self.count_documents().await?;
        let indexed_chunks = self.count_chunks().await?;
        let failed_items = self.failed_items().await?;
        let current_task = self.current_task().await?;

        Ok(IndexStatusView {
            indexed_docs: indexed_docs as usize,
            indexed_chunks: indexed_chunks as usize,
            scanned_docs: indexed_docs as usize,
            failed_files: failed_items.len(),
            current_task,
            failed_items,
        })
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

        for chunk in chunks {
            sqlx::query(
                r#"
                INSERT INTO chunks (id, document_id, heading, snippet, paragraph, page, score)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&document_id)
            .bind(&chunk.heading)
            .bind(&chunk.snippet)
            .bind(chunk.paragraph)
            .bind(chunk.page)
            .bind(chunk.score)
            .execute(&self.pool)
            .await?;
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
        progress: u8,
        scanned: usize,
        total: usize,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO current_task (id, label, details, progress, scanned, total)
            VALUES (1, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(label)
        .bind(details)
        .bind(progress as i64)
        .bind(scanned as i64)
        .bind(total as i64)
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
                progress INTEGER NOT NULL,
                scanned INTEGER NOT NULL,
                total INTEGER NOT NULL
            )
            "#,
        ];

        for statement in statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
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
            SELECT label, details, progress, scanned, total
            FROM current_task
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| CurrentTaskView {
            label: row.label,
            details: row.details,
            progress: row.progress as u8,
            scanned: row.scanned as usize,
            total: row.total as usize,
        }))
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
