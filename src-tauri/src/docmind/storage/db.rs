#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chrono::{TimeZone, Utc};
use dirs::{data_dir, download_dir, document_dir};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::docmind::file_ops;
use crate::docmind::models::{
    ChunkView, CurrentTaskView, DocumentView, FavoriteView, FailedFileView, HighlightSpan,
    IndexDirView, IndexStatusView, RecentDocumentView, SearchHistoryView, SearchResultView,
};
use crate::docmind::semantic::store as semantic_store;
use crate::docmind::search::normalize_query;
use crate::docmind::storage::fulltext::SearchIndex;
use crate::docmind::storage::indexer;
use crate::docmind::storage::types::{ChunkRecord, DocumentState, ExtractedDocument, IndexSettings};

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
    search_index: Arc<SearchIndex>,
    index_job_running: Arc<AtomicBool>,
}

impl Database {
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub(crate) fn try_begin_index_job(&self) -> bool {
        self.index_job_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(crate) fn end_index_job(&self) {
        self.index_job_running.store(false, Ordering::SeqCst);
    }
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
    modified_at: i64,
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
struct SearchHistoryRow {
    query: String,
    normalized_query: String,
    hit_count: i64,
    last_hit_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct RecentDocumentRow {
    path: String,
    title: String,
    file_name: String,
    ext: String,
    last_opened_at: i64,
    open_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct FavoriteRow {
    favorite_type: String,
    target: String,
    title: String,
    path: String,
    created_at: i64,
    updated_at: i64,
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
        let database = Self {
            pool,
            search_index,
            index_job_running: Arc::new(AtomicBool::new(false)),
        };
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
        database
            .ensure_embedding_models_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_vector_index_meta_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_history_tables()
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
                let _ = indexer::rebuild_all(&database, "bootstrap", |_| {}).await;
            } else if database.search_index.doc_count() == 0 {
                let _ = indexer::rebuild_all(&database, "bootstrap", |_| {}).await;
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
        let keyword_hits = self
            .search_index
            .search(query, limit.max(1))
            .map_err(sqlx::Error::Protocol)?;

        let semantic_hits = semantic_store::semantic_search_hits(self, query, limit.max(1))
            .await
            .unwrap_or_default();

        if keyword_hits.is_empty() && semantic_hits.is_empty() {
            return Ok(Vec::new());
        }

        let mut keyword_score_map = std::collections::HashMap::<String, f32>::new();
        for hit in &keyword_hits {
            keyword_score_map.insert(hit.chunk_id.clone(), hit.score);
        }

        let mut semantic_score_map = std::collections::HashMap::<String, f32>::new();
        for hit in &semantic_hits {
            semantic_score_map.insert(hit.chunk_id.clone(), hit.score);
        }

        let mut chunk_ids = keyword_hits
            .iter()
            .map(|hit| hit.chunk_id.clone())
            .collect::<Vec<_>>();
        for hit in &semantic_hits {
            if !keyword_score_map.contains_key(&hit.chunk_id) {
                chunk_ids.push(hit.chunk_id.clone());
            }
        }

        let rows = self.fetch_chunks_by_ids(&chunk_ids).await?;
        let mut rows_by_id = std::collections::HashMap::new();
        for row in rows {
            rows_by_id.insert(row.id.clone(), row);
        }

        let mut results = Vec::new();
        let normalized_terms = normalize_query(query);
        let now = Utc::now().timestamp();
        for chunk_id in chunk_ids {
            if let Some(row) = rows_by_id.remove(&chunk_id) {
                let keyword_score = keyword_score_map.get(&chunk_id).copied().unwrap_or(0.0);
                let semantic_score = semantic_score_map.get(&chunk_id).copied().unwrap_or(0.0);
                let (snippet, highlight_spans, snippet_window_start, snippet_window_end, snippet_source_len) =
                    build_search_snippet(&row.snippet, &normalized_terms, 220);
                let (matched_field, match_origin) = if keyword_score > 0.0 {
                    matched_field_and_origin(&row, &normalized_terms)
                } else if semantic_score > 0.0 {
                    ("semantic".to_string(), "语义命中".to_string())
                } else {
                    matched_field_and_origin(&row, &normalized_terms)
                };
                let base_score = if keyword_score > 0.0 {
                    keyword_score + semantic_score.max(0.0) * 0.25
                } else {
                    semantic_score.max(0.0)
                };
                let chunk_weight = row.score.clamp(0.25, 1.0);
                let score = boosted_search_score(
                    base_score * chunk_weight,
                    &match_origin,
                    row.modified_at,
                    now,
                );
                results.push(SearchResultView {
                    id: row.id,
                    file_name: row.file_name,
                    path: row.path,
                    ext: row.ext,
                    heading: row.heading,
                    snippet,
                    matched_field,
                    match_origin,
                    highlight_spans,
                    snippet_window_start,
                    snippet_window_end,
                    snippet_source_len,
                    paragraph: row.paragraph.map(|value| value as u32),
                    page: row.page.map(|value| value as u32),
                    modified: row.modified,
                    score,
                });
            }
        }

        results.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit.max(1));

        Ok(results)
    }

    pub async fn record_search_history(&self, query: &str, hit_count: usize) -> Result<(), sqlx::Error> {
        let normalized_query = normalize_query(query).join(" ");
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT INTO search_history
                (query, normalized_query, hit_count, created_at, last_hit_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(query) DO UPDATE SET
                normalized_query = excluded.normalized_query,
                hit_count = search_history.hit_count + excluded.hit_count,
                last_hit_at = excluded.last_hit_at
            "#,
        )
        .bind(query)
        .bind(normalized_query)
        .bind(hit_count as i64)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_search_history(&self, limit: i64) -> Result<Vec<SearchHistoryView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SearchHistoryRow>(
            r#"
            SELECT query, normalized_query, hit_count, last_hit_at
            FROM search_history
            ORDER BY last_hit_at DESC, hit_count DESC, query ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SearchHistoryView {
                query: row.query,
                normalized_query: row.normalized_query,
                hit_count: row.hit_count.max(0) as usize,
                last_hit_at: format_unix_ts(row.last_hit_at),
            })
            .collect())
    }

    pub async fn record_recent_document(
        &self,
        path: &str,
        title: &str,
        file_name: &str,
        ext: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT INTO recent_documents
                (path, title, file_name, ext, last_opened_at, open_count)
            VALUES (?, ?, ?, ?, ?, 1)
            ON CONFLICT(path) DO UPDATE SET
                title = excluded.title,
                file_name = excluded.file_name,
                ext = excluded.ext,
                last_opened_at = excluded.last_opened_at,
                open_count = recent_documents.open_count + 1
            "#,
        )
        .bind(path)
        .bind(title)
        .bind(file_name)
        .bind(ext)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_recent_documents(&self, limit: i64) -> Result<Vec<RecentDocumentView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RecentDocumentRow>(
            r#"
            SELECT path, title, file_name, ext, last_opened_at, open_count
            FROM recent_documents
            ORDER BY last_opened_at DESC, open_count DESC, path ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| RecentDocumentView {
                path: row.path,
                title: row.title,
                file_name: row.file_name,
                ext: row.ext,
                last_opened_at: format_unix_ts(row.last_opened_at),
                open_count: row.open_count.max(0) as usize,
            })
            .collect())
    }

    pub async fn list_favorites(&self, limit: i64) -> Result<Vec<FavoriteView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, FavoriteRow>(
            r#"
            SELECT favorite_type, target, title, path, created_at, updated_at
            FROM favorites
            ORDER BY updated_at DESC, created_at DESC, title ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| FavoriteView {
                favorite_type: row.favorite_type,
                target: row.target,
                title: row.title,
                path: row.path,
                created_at: format_unix_ts(row.created_at),
                updated_at: format_unix_ts(row.updated_at),
            })
            .collect())
    }

    pub async fn toggle_result_favorite(
        &self,
        path: &str,
        heading: &str,
        paragraph: Option<u32>,
        page: Option<u32>,
        file_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let target = favorite_result_target(path, heading, paragraph, page);
        let now = current_unix_ts();
        let existing = sqlx::query("SELECT target FROM favorites WHERE target = ?")
            .bind(&target)
            .fetch_optional(&self.pool)
            .await?;

        if existing.is_some() {
            sqlx::query("DELETE FROM favorites WHERE target = ?")
                .bind(&target)
                .execute(&self.pool)
                .await?;
            return Ok(false);
        }

        sqlx::query(
            r#"
            INSERT INTO favorites
                (target, favorite_type, title, path, created_at, updated_at)
            VALUES (?, 'result', ?, ?, ?, ?)
            "#,
        )
        .bind(&target)
        .bind(if heading.trim().is_empty() { file_name } else { heading })
        .bind(path)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(true)
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
        self.clear_directory_documents(path).await?;
        let _ = self.clear_directory_failed_files(path).await;
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

        Ok(())
    }

    pub(crate) async fn clear_document_by_path(&self, path: &str) -> Result<(), sqlx::Error> {
        self.search_index
            .delete_document(path)
            .map_err(sqlx::Error::Protocol)?;
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

        if let Err(error) = semantic_store::upsert_document_embeddings(self, &document_id, document, chunks).await {
            eprintln!(
                "[DocMind] semantic embedding update failed for {}: {}",
                document.path, error
            );
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

        sqlx::query("DELETE FROM chunk_embeddings")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"
            UPDATE vector_index_meta
            SET chunk_count = 0,
                last_indexed_at = 0,
                last_error = '',
                status = 'idle'
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;
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
            r#"
            CREATE TABLE IF NOT EXISTS embedding_models (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                model_path TEXT NOT NULL DEFAULT '',
                dimension INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                available INTEGER NOT NULL DEFAULT 0,
                is_default INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'unknown',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS chunk_embeddings (
                chunk_id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                vector_json TEXT NOT NULL,
                dimension INTEGER NOT NULL,
                text_hash TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'ready',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS vector_index_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                model_id TEXT NOT NULL,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                last_indexed_at INTEGER NOT NULL DEFAULT 0,
                last_error TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'idle'
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS search_history (
                query TEXT PRIMARY KEY,
                normalized_query TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                last_hit_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recent_documents (
                path TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                file_name TEXT NOT NULL,
                ext TEXT NOT NULL,
                last_opened_at INTEGER NOT NULL DEFAULT 0,
                open_count INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS favorites (
                target TEXT PRIMARY KEY,
                favorite_type TEXT NOT NULL,
                title TEXT NOT NULL,
                path TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
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

    async fn ensure_embedding_models_row(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM embedding_models").await?;
        if count == 0 {
            let now = current_unix_ts();
            sqlx::query(
                r#"
                INSERT INTO embedding_models
                    (id, name, provider, model_path, dimension, enabled, available, is_default, status, created_at, updated_at)
                VALUES
                    ('default-local-embedding', 'BAAI/bge-small-zh-v1.5', 'fastembed', '', 512, 1, 0, 1, 'unknown', ?, ?)
                "#,
            )
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        } else {
            let now = current_unix_ts();
            sqlx::query(
                r#"
                UPDATE embedding_models
                SET provider = 'fastembed',
                    dimension = 512,
                    updated_at = ?
                WHERE name = 'BAAI/bge-small-zh-v1.5'
                "#,
            )
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn ensure_vector_index_meta_row(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM vector_index_meta").await?;
        if count == 0 {
            sqlx::query(
                r#"
                INSERT INTO vector_index_meta (id, model_id, chunk_count, last_indexed_at, last_error, status)
                VALUES (1, 'default-local-embedding', 0, 0, '', 'idle')
                "#,
            )
            .execute(&self.pool)
            .await?;
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

    async fn ensure_history_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS search_history (
                query TEXT PRIMARY KEY,
                normalized_query TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                last_hit_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS recent_documents (
                path TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                file_name TEXT NOT NULL,
                ext TEXT NOT NULL,
                last_opened_at INTEGER NOT NULL DEFAULT 0,
                open_count INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS favorites (
                target TEXT PRIMARY KEY,
                favorite_type TEXT NOT NULL,
                title TEXT NOT NULL,
                path TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
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
                d.modified_at,
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

fn matched_field_and_origin(row: &SearchRow, terms: &[String]) -> (String, String) {
    let candidates = [
        ("snippet", "正文摘要命中", row.snippet.as_str()),
        ("heading", "标题命中", row.heading.as_str()),
        ("filename", "文件名命中", row.file_name.as_str()),
        ("path", "路径命中", row.path.as_str()),
    ];

    for (field, origin, text) in candidates {
        let spans = highlight_spans(text, terms);
        if !spans.is_empty() {
            return (field.to_string(), origin.to_string());
        }
    }

    ("snippet".to_string(), "正文摘要命中".to_string())
}

fn highlight_spans(text: &str, terms: &[String]) -> Vec<HighlightSpan> {
    let lower_text = text.to_lowercase();
    let mut spans = Vec::new();

    for term in terms {
        let needle = term.trim();
        if needle.is_empty() {
            continue;
        }

        let normalized_needle = needle.to_lowercase();
        for (start, _) in lower_text.match_indices(&normalized_needle) {
            let start_char = lower_text[..start].chars().count();
            let end_char = start_char + normalized_needle.chars().count();
            spans.push(HighlightSpan {
                start: start_char,
                end: end_char,
            });
        }
    }

    if spans.is_empty() {
        return spans;
    }

    spans.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.start <= last.end {
                last.end = last.end.max(span.end);
                continue;
            }
        }
        merged.push(span);
    }

    merged
}

fn build_search_snippet(
    text: &str,
    terms: &[String],
    max_chars: usize,
) -> (String, Vec<HighlightSpan>, usize, usize, usize) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (String::new(), Vec::new(), 0, 0, 0);
    }

    let spans = highlight_spans(trimmed, terms);
    if spans.is_empty() {
        let total = trimmed.chars().count();
        let snippet = truncate_by_chars(trimmed, max_chars);
        let end = total.min(max_chars);
        return (snippet, Vec::new(), 0, end, total);
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let total_chars = chars.len();
    if total_chars <= max_chars {
        return (trimmed.to_string(), spans, 0, total_chars, total_chars);
    }

    let focus_start = spans.first().map(|span| span.start).unwrap_or(0);
    let focus_end = spans.last().map(|span| span.end).unwrap_or(total_chars);
    let target = max_chars.min(total_chars);
    let before = target.saturating_mul(2) / 5;
    let after = target.saturating_sub(before);

    let mut snippet_start = focus_start.saturating_sub(before);
    let mut snippet_end = (focus_end + after).min(total_chars);

    if snippet_end.saturating_sub(snippet_start) < target {
        let shortfall = target - (snippet_end - snippet_start);
        let left_pad = shortfall / 2;
        let right_pad = shortfall - left_pad;
        snippet_start = snippet_start.saturating_sub(left_pad);
        snippet_end = (snippet_end + right_pad).min(total_chars);
    }

    if snippet_end.saturating_sub(snippet_start) > target {
        snippet_end = snippet_start + target;
    }

    let leading_ellipsis = snippet_start > 0;
    let trailing_ellipsis = snippet_end < total_chars;
    let mut snippet = slice_chars(trimmed, snippet_start, snippet_end);
    if leading_ellipsis {
        snippet = format!("…{snippet}");
    }
    if trailing_ellipsis {
        snippet.push('…');
    }

    let prefix_offset = if leading_ellipsis { 1 } else { 0 };
    let mut adjusted_spans = Vec::with_capacity(spans.len());
    for span in spans {
        let start = span.start.max(snippet_start);
        let end = span.end.min(snippet_end);
        if end <= start {
            continue;
        }
        adjusted_spans.push(HighlightSpan {
            start: start - snippet_start + prefix_offset,
            end: end - snippet_start + prefix_offset,
        });
    }

    (snippet, adjusted_spans, snippet_start, snippet_end, total_chars)
}

fn boosted_search_score(base_score: f32, match_origin: &str, modified_at: i64, now: i64) -> f32 {
    let field_boost = match match_origin {
        "文件名命中" => 0.45,
        "标题命中" => 0.32,
        "正文摘要命中" => 0.18,
        "路径命中" => 0.08,
        _ => 0.12,
    };

    let recency_boost = if modified_at > 0 && now > modified_at {
        let age_days = ((now - modified_at) as f32 / 86_400.0).min(3_650.0);
        (1.0 / (1.0 + age_days / 30.0)) * 0.15
    } else {
        0.05
    };

    base_score + field_boost + recency_boost
}

fn truncate_by_chars(text: &str, limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        return text.to_string();
    }

    let mut snippet: String = chars.iter().take(limit).collect();
    snippet.push('…');
    snippet
}

fn slice_chars(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end.saturating_sub(start)).collect()
}

fn favorite_result_target(
    path: &str,
    heading: &str,
    paragraph: Option<u32>,
    page: Option<u32>,
) -> String {
    format!(
        "result|{}|{}|{}|{}",
        path,
        heading.trim(),
        paragraph.map(|value| value.to_string()).unwrap_or_default(),
        page.map(|value| value.to_string()).unwrap_or_default()
    )
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
