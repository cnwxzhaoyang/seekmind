#![allow(dead_code)]

/**
 * @author MorningSun
 * @CreatedDate 2026/06/03
 * @Description DocMind 本地 SQLite 存储与查询实现。
 */
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use chrono::{TimeZone, Utc};
use dirs::data_dir;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::docmind::file_ops;
use crate::docmind::models::{
    ChunkView, CollectionItemView, CollectionView, CurrentTaskView, DocumentView, FailedFileView,
    FavoriteView, HighlightSpan, IndexDirView, IndexStatusView, PreviewBlockView, QaAnswerView,
    QaHistoryView, QaMessageView, QaModelProfileUpsertView, QaModelProfileView, QaRetrievalView,
    QaSessionView, QaSourceView, RecentDocumentView, RecentViewEntry, SearchHistoryView,
    SearchResultView, TagView,
};
use crate::docmind::search::{normalize_query, rewrite_query_terms, rewrite_search_text};
use crate::docmind::semantic::store as semantic_store;
use crate::docmind::storage::fulltext::SearchIndex;
use crate::docmind::storage::types::{
    ChunkRecord, CollectionItemInput, CollectionPatchInput, DocumentState, ExtractedDocument,
    IndexSettings, NetworkProxySettings, QaSettings, TagPatchInput,
};

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
struct DocumentPathRow {
    path: String,
    dir_path: String,
    chunks: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct FulltextDocumentRow {
    id: String,
    dir_path: String,
    path: String,
    file_name: String,
    ext: String,
    file_size: i64,
    modified_at: i64,
    content_hash: String,
    modified: String,
    content: String,
}

#[derive(Debug, sqlx::FromRow)]
struct FulltextChunkRow {
    heading: String,
    snippet: String,
    paragraph: Option<i64>,
    page: Option<i64>,
    score: f32,
}

#[derive(Debug, Clone)]
struct DirectoryAggregate {
    path: String,
    enabled: bool,
    docs: usize,
    chunks: usize,
    status: String,
    is_explicit: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchRow {
    id: String,
    document_id: String,
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
    block_indexes_json: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SearchDebugData {
    pub(crate) hits: Vec<SearchResultView>,
    pub(crate) keyword_hit_count: usize,
    pub(crate) semantic_hit_count: usize,
    pub(crate) semantic_candidate_count: usize,
    pub(crate) semantic_filtered_count: usize,
    pub(crate) semantic_enabled: bool,
    pub(crate) semantic_weight: f32,
    pub(crate) semantic_threshold: f32,
    pub(crate) rewritten_terms: Vec<String>,
    pub(crate) rewritten_query: String,
    pub(crate) history_terms: Vec<String>,
    pub(crate) history_rewrite_applied: bool,
    pub(crate) expanded_query: String,
    pub(crate) semantic_fallback: bool,
    pub(crate) semantic_fallback_reason: String,
    pub(crate) search_mode: String,
}

#[derive(Debug, Clone)]
struct SearchResultCandidate {
    result: SearchResultView,
    raw_score: f32,
    final_score: f32,
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
    block_indexes_json: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BlockRow {
    block_index: i64,
    block_type: String,
    text: String,
    heading: String,
    level: Option<i64>,
    page: Option<i64>,
    language: String,
    markdown: String,
    html: String,
    asset_path: String,
    alt_text: String,
    caption: String,
    ocr_text: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BlockWithDocumentRow {
    document_id: String,
    block_index: i64,
    block_type: String,
    text: String,
    heading: String,
    level: Option<i64>,
    page: Option<i64>,
    language: String,
    markdown: String,
    html: String,
    asset_path: String,
    alt_text: String,
    caption: String,
    ocr_text: String,
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
struct RecentViewRow {
    target_type: String,
    target_id: String,
    title: String,
    path: String,
    viewed_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TagRow {
    id: String,
    name: String,
    color: String,
    target_count: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct TargetTagRow {
    id: String,
    name: String,
    color: String,
    created_at: i64,
    updated_at: i64,
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
struct CollectionRow {
    id: String,
    name: String,
    description: String,
    color: String,
    sort_order: i64,
    item_count: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct CollectionItemRow {
    id: String,
    collection_id: String,
    item_type: String,
    document_id: String,
    chunk_id: String,
    qa_session_id: String,
    qa_message_id: String,
    title: String,
    path: String,
    title_path: String,
    snippet: String,
    note: String,
    source_meta_json: String,
    sort_order: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct QaSettingsRow {
    enabled: i64,
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
    temperature: f32,
    max_output_tokens: i64,
    context_chunk_limit: i64,
    context_token_budget: i64,
    min_evidence_count: i64,
    min_retrieval_score: f32,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct NetworkProxySettingsRow {
    enabled: i64,
    proxy_url: String,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct QaModelProfileRow {
    id: String,
    name: String,
    provider: String,
    base_url: String,
    api_key: String,
    model: String,
    enabled: i64,
    is_default: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct QaHistoryRow {
    id: String,
    question: String,
    answer: String,
    state: String,
    sources_json: String,
    retrieval_json: String,
    model: String,
    error: String,
    created_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct QaSessionRow {
    id: String,
    title: String,
    message_count: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct QaMessageRow {
    id: String,
    session_id: String,
    question: String,
    answer: String,
    state: String,
    sources_json: String,
    retrieval_json: String,
    model: String,
    error: String,
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
    started_at: i64,
    progress: i64,
    scanned: i64,
    total: i64,
    succeeded: i64,
    failed: i64,
    updated: i64,
    skipped: i64,
    deleted: i64,
    warning: Option<String>,
    pause_requested: i64,
}

impl Database {
    pub async fn open_or_init() -> Result<Self, String> {
        let path = database_path();
        eprintln!("[DocMind] SQLite database path: {}", path.display());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
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
            .ensure_index_settings_columns()
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
        database
            .ensure_qa_settings_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_network_proxy_settings_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_collections_seed()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_qa_model_profiles_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_chunks_block_indexes_column()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_document_blocks_columns()
            .await
            .map_err(|error| error.to_string())?;

        if documents_migrated {
            database
                .clear_all_index_data()
                .await
                .map_err(|error| error.to_string())?;
        }

        Ok(database)
    }

    pub async fn get_index_settings(&self) -> Result<IndexSettings, sqlx::Error> {
        #[derive(Debug, sqlx::FromRow)]
        struct IndexSettingsRow {
            exclude_dirs: String,
            exclude_exts: String,
            max_file_size_mb: i64,
            semantic_search_enabled: i64,
            semantic_weight: f32,
            semantic_threshold: f32,
            title_weight: f32,
            filename_weight: f32,
            preference_weight: f32,
            prefer_favorites_enabled: i64,
            prefer_recent_enabled: i64,
            prefer_history_enabled: i64,
        }

        let row = sqlx::query_as::<_, IndexSettingsRow>(
            r#"
            SELECT exclude_dirs, exclude_exts, max_file_size_mb, semantic_search_enabled, semantic_weight, semantic_threshold, title_weight, filename_weight, preference_weight, prefer_favorites_enabled, prefer_recent_enabled, prefer_history_enabled
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
                semantic_search_enabled: row.semantic_search_enabled != 0,
                semantic_weight: row.semantic_weight.clamp(0.0, 1.0),
                semantic_threshold: row.semantic_threshold.clamp(-1.0, 1.0),
                title_weight: row.title_weight.clamp(0.0, 3.0),
                filename_weight: row.filename_weight.clamp(0.0, 3.0),
                preference_weight: row.preference_weight.clamp(0.0, 3.0),
                prefer_favorites_enabled: row.prefer_favorites_enabled != 0,
                prefer_recent_enabled: row.prefer_recent_enabled != 0,
                prefer_history_enabled: row.prefer_history_enabled != 0,
            })
        } else {
            Ok(IndexSettings {
                exclude_dirs: default_exclude_dirs(),
                exclude_exts: Vec::new(),
                max_file_size_mb: 50,
                semantic_search_enabled: true,
                semantic_weight: 0.25,
                semantic_threshold: 0.2,
                title_weight: 1.0,
                filename_weight: 1.0,
                preference_weight: 1.0,
                prefer_favorites_enabled: true,
                prefer_recent_enabled: true,
                prefer_history_enabled: true,
            })
        }
    }

    pub async fn save_index_settings(&self, settings: &IndexSettings) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_settings
                (id, exclude_dirs, exclude_exts, max_file_size_mb, semantic_search_enabled, semantic_weight, semantic_threshold, title_weight, filename_weight, preference_weight, prefer_favorites_enabled, prefer_recent_enabled, prefer_history_enabled)
            VALUES (
                1,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?
            )
            "#,
        )
        .bind(serde_json::to_string(&settings.exclude_dirs).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&settings.exclude_exts).unwrap_or_else(|_| "[]".to_string()))
        .bind(settings.max_file_size_mb as i64)
        .bind(settings.semantic_search_enabled as i64)
        .bind(settings.semantic_weight)
        .bind(settings.semantic_threshold)
        .bind(settings.title_weight)
        .bind(settings.filename_weight)
        .bind(settings.preference_weight)
        .bind(settings.prefer_favorites_enabled as i64)
        .bind(settings.prefer_recent_enabled as i64)
        .bind(settings.prefer_history_enabled as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_index_dirs(&self) -> Result<Vec<IndexDirView>, sqlx::Error> {
        let explicit_rows = sqlx::query_as::<_, IndexDirRow>(
            r#"
            SELECT path, enabled, docs, chunks, status
            FROM index_dirs
            ORDER BY path
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let document_rows = sqlx::query_as::<_, DocumentPathRow>(
            r#"
            SELECT
                d.path,
                d.dir_path,
                COUNT(c.id) AS chunks
            FROM documents d
            LEFT JOIN chunks c ON c.document_id = d.id
            GROUP BY d.id, d.path, d.dir_path
            ORDER BY d.path
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut nodes = std::collections::HashMap::<String, DirectoryAggregate>::new();
        let explicit_paths = explicit_rows
            .iter()
            .map(|row| normalize_directory_path(&row.path))
            .collect::<Vec<_>>();

        for row in explicit_rows {
            let path = normalize_directory_path(&row.path);
            nodes.insert(
                path.clone(),
                DirectoryAggregate {
                    path,
                    enabled: row.enabled != 0,
                    docs: 0,
                    chunks: 0,
                    status: row.status,
                    is_explicit: true,
                },
            );
        }

        let mut sorted_explicit_paths = explicit_paths.clone();
        sorted_explicit_paths
            .sort_by(|left, right| left.len().cmp(&right.len()).then(left.cmp(right)));

        for doc in document_rows {
            let doc_path = normalize_directory_path(&doc.path);
            if doc_path.is_empty() {
                continue;
            }

            let doc_parent = normalize_directory_path(
                &std::path::Path::new(&doc_path)
                    .parent()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_string(),
            );
            if doc_parent.is_empty() {
                continue;
            }

            let matching_roots = sorted_explicit_paths
                .iter()
                .filter(|root| {
                    if is_virtual_directory(root) {
                        doc.dir_path == root.as_str()
                    } else {
                        is_path_within_dir(&doc_path, root)
                    }
                })
                .cloned()
                .collect::<Vec<_>>();

            if matching_roots.is_empty() {
                continue;
            }

            for root in matching_roots {
                if is_virtual_directory(&root) {
                    if doc.dir_path == root {
                        let entry =
                            nodes
                                .entry(root.clone())
                                .or_insert_with(|| DirectoryAggregate {
                                    path: root.clone(),
                                    enabled: true,
                                    docs: 0,
                                    chunks: 0,
                                    status: "indexed".to_string(),
                                    is_explicit: true,
                                });
                        entry.docs += 1;
                        entry.chunks += doc.chunks as usize;
                    }
                    continue;
                }

                let mut current = doc_parent.clone();
                loop {
                    if !is_path_within_dir(&current, &root) && current != root {
                        break;
                    }

                    let entry =
                        nodes
                            .entry(current.clone())
                            .or_insert_with(|| DirectoryAggregate {
                                path: current.clone(),
                                enabled: false,
                                docs: 0,
                                chunks: 0,
                                status: "empty".to_string(),
                                is_explicit: false,
                            });
                    entry.docs += 1;
                    entry.chunks += doc.chunks as usize;

                    if current == root {
                        break;
                    }

                    let parent = normalize_directory_path(
                        &std::path::Path::new(&current)
                            .parent()
                            .and_then(|value| value.to_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                    if parent.is_empty() || parent == current {
                        break;
                    }
                    current = parent;
                }
            }
        }

        let mut node_paths = nodes.keys().cloned().collect::<Vec<_>>();
        node_paths.sort_by(|left, right| left.len().cmp(&right.len()).then(left.cmp(right)));

        let mut enabled_snapshot = nodes
            .iter()
            .map(|(path, node)| (path.clone(), node.enabled))
            .collect::<std::collections::HashMap<_, _>>();

        for path in &node_paths {
            if let Some(node) = nodes.get_mut(path) {
                if !node.is_explicit {
                    let parent = normalize_directory_path(
                        &std::path::Path::new(path)
                            .parent()
                            .and_then(|value| value.to_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                    node.enabled = enabled_snapshot.get(&parent).copied().unwrap_or(false);
                }

                if node.status.trim().is_empty() {
                    node.status = if node.docs > 0 {
                        "indexed".to_string()
                    } else {
                        "empty".to_string()
                    };
                }

                enabled_snapshot.insert(path.clone(), node.enabled);
            }
        }

        let mut rows = nodes
            .into_values()
            .map(|node| IndexDirView {
                path: node.path,
                enabled: node.enabled,
                docs: node.docs,
                chunks: node.chunks,
                status: node.status,
                is_explicit: node.is_explicit,
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            left.path
                .len()
                .cmp(&right.path.len())
                .then(left.path.cmp(&right.path))
        });
        Ok(rows)
    }

    pub async fn search_documents(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResultView>, sqlx::Error> {
        Ok(self.build_search_results(query, limit).await?.hits)
    }

    pub(crate) async fn fulltext_repair_needed(&self) -> Result<bool, sqlx::Error> {
        let sqlite_chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
            .fetch_one(&self.pool)
            .await?;
        Ok(sqlite_chunks > 0 && self.tantivy_document_count() == 0)
    }

    pub(crate) async fn repair_empty_fulltext_index<F>(
        &self,
        mut on_progress: F,
    ) -> Result<(), sqlx::Error>
    where
        F: FnMut(usize, usize, String) + Send,
    {
        let sqlite_chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
            .fetch_one(&self.pool)
            .await?;
        if sqlite_chunks == 0 || self.tantivy_document_count() > 0 {
            return Ok(());
        }

        eprintln!("[DocMind] repairing empty Tantivy index from SQLite chunks={sqlite_chunks}");
        self.search_index
            .clear_all()
            .map_err(sqlx::Error::Protocol)?;

        let documents = sqlx::query_as::<_, FulltextDocumentRow>(
            r#"
            SELECT id, dir_path, path, file_name, ext, file_size, modified_at, content_hash, modified, content
            FROM documents
            ORDER BY rowid
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        let total = documents.len();

        for (index, document) in documents.into_iter().enumerate() {
            let chunks = sqlx::query_as::<_, FulltextChunkRow>(
                r#"
                SELECT heading, snippet, paragraph, page, score
                FROM chunks
                WHERE document_id = ?
                ORDER BY rowid
                "#,
            )
            .bind(&document.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| ChunkRecord {
                heading: row.heading,
                snippet: row.snippet,
                paragraph: row.paragraph,
                page: row.page,
                score: row.score,
                block_indexes: Vec::new(),
            })
            .collect::<Vec<_>>();

            let extracted = ExtractedDocument {
                dir_path: document.dir_path,
                path: document.path,
                file_name: document.file_name,
                ext: document.ext,
                file_size: document.file_size,
                modified_at: document.modified_at,
                content_hash: document.content_hash,
                modified: document.modified,
                content: document.content,
            };

            self.search_index
                .index_document(&document.id, &extracted, &chunks)
                .map_err(sqlx::Error::Protocol)?;
            on_progress(index + 1, total, extracted.file_name);
        }

        eprintln!(
            "[DocMind] repaired Tantivy index docs={}",
            self.tantivy_document_count()
        );
        Ok(())
    }

    pub async fn record_search_history(
        &self,
        query: &str,
        hit_count: usize,
    ) -> Result<(), sqlx::Error> {
        let normalized_query = normalize_query(query).join(" ");
        if normalized_query.trim().is_empty() {
            return Ok(());
        }

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

    pub async fn list_search_history(
        &self,
        limit: i64,
    ) -> Result<Vec<SearchHistoryView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SearchHistoryRow>(
            r#"
            SELECT query, normalized_query, hit_count, last_hit_at
            FROM search_history
            WHERE trim(query) <> '' AND trim(normalized_query) <> ''
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

    pub async fn remove_search_history(&self, query: &str) -> Result<(), sqlx::Error> {
        if query.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM search_history WHERE query = ?")
            .bind(query)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn derive_history_terms(
        &self,
        current_terms: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        let history = self.list_search_history(30).await?;
        let current_set = current_terms
            .iter()
            .map(|term| term.trim().to_lowercase())
            .filter(|term| !term.is_empty())
            .collect::<std::collections::HashSet<_>>();

        let mut term_counts = std::collections::HashMap::<String, usize>::new();
        for item in history {
            let weight = item.hit_count.max(1);
            for term in item.normalized_query.split_whitespace() {
                let normalized = term.trim().to_lowercase();
                if normalized.is_empty()
                    || current_set.contains(&normalized)
                    || normalized.len() < 2
                {
                    continue;
                }
                *term_counts.entry(normalized).or_insert(0) += weight;
            }
        }

        let mut terms = term_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .collect::<Vec<_>>();
        terms.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        Ok(terms.into_iter().take(4).map(|(term, _)| term).collect())
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

    pub async fn get_qa_settings(&self) -> Result<QaSettings, sqlx::Error> {
        let row = sqlx::query_as::<_, QaSettingsRow>(
            r#"
            SELECT enabled, provider, base_url, api_key, model, temperature, max_output_tokens, context_chunk_limit, context_token_budget, min_evidence_count, min_retrieval_score, updated_at
            FROM qa_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(QaSettings {
                enabled: row.enabled != 0,
                provider: row.provider,
                base_url: row.base_url,
                api_key: row.api_key,
                model: row.model,
                temperature: row.temperature.clamp(0.0, 2.0),
                max_output_tokens: row.max_output_tokens.max(1) as usize,
                context_chunk_limit: row.context_chunk_limit.max(1) as usize,
                context_token_budget: row.context_token_budget.max(1) as usize,
                min_evidence_count: row.min_evidence_count.max(1) as usize,
                min_retrieval_score: row.min_retrieval_score,
            })
        } else {
            Ok(default_qa_settings())
        }
    }

    pub async fn get_qa_settings_updated_at(&self) -> Result<String, sqlx::Error> {
        let updated_at = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT updated_at
            FROM qa_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);

        Ok(format_unix_ts(updated_at))
    }

    pub async fn get_network_proxy_settings(&self) -> Result<NetworkProxySettings, sqlx::Error> {
        let row = sqlx::query_as::<_, NetworkProxySettingsRow>(
            r#"
            SELECT enabled, proxy_url, updated_at
            FROM network_proxy_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(NetworkProxySettings {
                enabled: row.enabled != 0,
                proxy_url: row.proxy_url,
            })
        } else {
            Ok(default_network_proxy_settings())
        }
    }

    pub async fn get_network_proxy_settings_updated_at(&self) -> Result<String, sqlx::Error> {
        let updated_at = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT updated_at
            FROM network_proxy_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);

        Ok(format_unix_ts(updated_at))
    }

    pub async fn save_qa_settings(&self, settings: &QaSettings) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_settings
                (id, enabled, provider, base_url, api_key, model, temperature, max_output_tokens, context_chunk_limit, context_token_budget, min_evidence_count, min_retrieval_score, updated_at)
            VALUES (
                1,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?
            )
            "#,
        )
        .bind(settings.enabled as i64)
        .bind(settings.provider.trim())
        .bind(settings.base_url.trim())
        .bind(settings.api_key.as_str())
        .bind(settings.model.trim())
        .bind(settings.temperature)
        .bind(settings.max_output_tokens as i64)
        .bind(settings.context_chunk_limit as i64)
        .bind(settings.context_token_budget as i64)
        .bind(settings.min_evidence_count as i64)
        .bind(settings.min_retrieval_score)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_qa_model_profiles(&self) -> Result<Vec<QaModelProfileView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            ORDER BY is_default DESC, updated_at DESC, created_at DESC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(qa_model_profile_row_to_view).collect())
    }

    pub async fn get_qa_model_profile(
        &self,
        profile_id: &str,
    ) -> Result<Option<QaModelProfileView>, sqlx::Error> {
        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(qa_model_profile_row_to_view))
    }

    pub async fn save_qa_model_profile(
        &self,
        profile: &QaModelProfileUpsertView,
    ) -> Result<QaModelProfileView, sqlx::Error> {
        let now = current_unix_ts();
        let id = profile
            .id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let existing = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_optional(&self.pool)
        .await?;
        let created_at = existing.as_ref().map(|item| item.created_at).unwrap_or(now);

        if profile.is_default {
            sqlx::query("UPDATE qa_model_profiles SET is_default = 0, updated_at = ?")
                .bind(now)
                .execute(&self.pool)
                .await?;
        }

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_model_profiles
                (id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(profile.name.trim())
        .bind(profile.provider.trim())
        .bind(profile.base_url.trim())
        .bind(profile.api_key.as_str())
        .bind(profile.model.trim())
        .bind(profile.enabled as i64)
        .bind(profile.is_default as i64)
        .bind(created_at)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;

        Ok(qa_model_profile_row_to_view(row))
    }

    pub async fn remove_qa_model_profile(&self, profile_id: &str) -> Result<(), sqlx::Error> {
        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        let was_default = row.map(|item| item.is_default != 0).unwrap_or(false);
        sqlx::query("DELETE FROM qa_model_profiles WHERE id = ?")
            .bind(profile_id)
            .execute(&self.pool)
            .await?;

        if was_default {
            let next_default = sqlx::query_scalar::<_, String>(
                r#"
                SELECT id
                FROM qa_model_profiles
                ORDER BY updated_at DESC, created_at DESC, name ASC
                LIMIT 1
                "#,
            )
            .fetch_optional(&self.pool)
            .await?;

            if let Some(next_default) = next_default {
                self.set_default_qa_model_profile(&next_default).await?;
            }
        }

        Ok(())
    }

    pub async fn set_default_qa_model_profile(
        &self,
        profile_id: &str,
    ) -> Result<QaModelProfileView, sqlx::Error> {
        let now = current_unix_ts();
        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Err(sqlx::Error::RowNotFound);
        };

        sqlx::query("UPDATE qa_model_profiles SET is_default = 0, updated_at = ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        sqlx::query("UPDATE qa_model_profiles SET is_default = 1, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(profile_id)
            .execute(&self.pool)
            .await?;

        Ok(qa_model_profile_row_to_view(QaModelProfileRow {
            is_default: 1,
            updated_at: now,
            ..row
        }))
    }

    pub async fn list_qa_history(&self, limit: i64) -> Result<Vec<QaHistoryView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, QaHistoryRow>(
            r#"
            SELECT id, question, answer, state, sources_json, retrieval_json, model, error, created_at
            FROM qa_history
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| qa_history_row_to_view(row))
            .collect())
    }

    pub async fn remove_qa_history(&self, id: &str) -> Result<(), sqlx::Error> {
        if id.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM qa_history WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn record_qa_history(&self, answer: &QaAnswerView) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        let sources_json =
            serde_json::to_string(&answer.sources).unwrap_or_else(|_| "[]".to_string());
        let retrieval_json =
            serde_json::to_string(&answer.retrieval).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_history
                (id, question, answer, state, sources_json, retrieval_json, model, error, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&answer.id)
        .bind(&answer.question)
        .bind(&answer.answer)
        .bind(&answer.state)
        .bind(sources_json)
        .bind(retrieval_json)
        .bind(&answer.model)
        .bind(answer.error.as_deref().unwrap_or(""))
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_recent_documents(
        &self,
        limit: i64,
    ) -> Result<Vec<RecentDocumentView>, sqlx::Error> {
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

    pub async fn remove_recent_document(&self, path: &str) -> Result<(), sqlx::Error> {
        if path.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM recent_documents WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn record_recent_view(
        &self,
        target_type: &str,
        target_id: &str,
        title: &str,
        path: &str,
    ) -> Result<(), sqlx::Error> {
        let target_type = normalize_recent_view_target_type(target_type);
        let target_id = target_id.trim();
        if target_type.is_empty() || target_id.is_empty() {
            return Ok(());
        }

        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT INTO recent_views
                (target_type, target_id, title, path, viewed_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(target_type, target_id) DO UPDATE SET
                title = excluded.title,
                path = excluded.path,
                viewed_at = excluded.viewed_at
            "#,
        )
        .bind(target_type)
        .bind(target_id)
        .bind(title.trim())
        .bind(path.trim())
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_recent_views(&self, limit: i64) -> Result<Vec<RecentViewEntry>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RecentViewRow>(
            r#"
            SELECT target_type, target_id, title, path, viewed_at
            FROM recent_views
            ORDER BY viewed_at DESC, title ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| RecentViewEntry {
                target_type: row.target_type,
                target_id: row.target_id,
                title: row.title,
                path: row.path,
                viewed_at: format_unix_ts(row.viewed_at),
            })
            .collect())
    }

    pub async fn list_tags(&self) -> Result<Vec<TagView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                COUNT(it.id) AS target_count,
                t.created_at,
                t.updated_at
            FROM tags t
            LEFT JOIN item_tags it ON it.tag_id = t.id
            GROUP BY t.id, t.name, t.color, t.created_at, t.updated_at
            ORDER BY t.updated_at DESC, t.created_at DESC, t.name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(tag_row_to_view).collect())
    }

    pub async fn create_tag(&self, name: &str, color: &str) -> Result<TagView, sqlx::Error> {
        let normalized_name = normalize_tag_name(name);
        let normalized_color = normalize_tag_color(color);
        let now = current_unix_ts();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO tags (id, name, color, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&normalized_name)
        .bind(&normalized_color)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(TagView {
            id,
            name: normalized_name,
            color: normalized_color,
            target_count: 0,
            created_at: format_unix_ts(now),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn update_tag(
        &self,
        tag_id: &str,
        patch: &TagPatchInput,
    ) -> Result<TagView, sqlx::Error> {
        let existing = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                COUNT(it.id) AS target_count,
                t.created_at,
                t.updated_at
            FROM tags t
            LEFT JOIN item_tags it ON it.tag_id = t.id
            WHERE t.id = ?
            GROUP BY t.id, t.name, t.color, t.created_at, t.updated_at
            "#,
        )
        .bind(tag_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(existing) = existing else {
            return Err(sqlx::Error::RowNotFound);
        };

        let name = patch
            .name
            .as_deref()
            .map(normalize_tag_name)
            .unwrap_or(existing.name);
        let color = patch
            .color
            .as_deref()
            .map(normalize_tag_color)
            .unwrap_or(existing.color);
        let now = current_unix_ts();
        sqlx::query(
            r#"
            UPDATE tags
            SET name = ?, color = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&name)
        .bind(&color)
        .bind(now)
        .bind(tag_id)
        .execute(&self.pool)
        .await?;

        Ok(TagView {
            id: existing.id,
            name,
            color,
            target_count: existing.target_count.max(0) as usize,
            created_at: format_unix_ts(existing.created_at),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn delete_tag(&self, tag_id: &str) -> Result<(), sqlx::Error> {
        if tag_id.trim().is_empty() {
            return Ok(());
        }
        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(tag_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_target_tags(
        &self,
        target_type: &str,
        target_id: &str,
    ) -> Result<Vec<TagView>, sqlx::Error> {
        let target_type = normalize_tag_target_type(target_type);
        if target_type.is_empty() || target_id.trim().is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, TargetTagRow>(
            r#"
            SELECT t.id, t.name, t.color, t.created_at, t.updated_at
            FROM item_tags it
            JOIN tags t ON t.id = it.tag_id
            WHERE it.target_type = ? AND it.target_id = ?
            ORDER BY t.updated_at DESC, t.name ASC
            "#,
        )
        .bind(target_type)
        .bind(target_id.trim())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| TagView {
                id: row.id,
                name: row.name,
                color: row.color,
                target_count: 0,
                created_at: format_unix_ts(row.created_at),
                updated_at: format_unix_ts(row.updated_at),
            })
            .collect())
    }

    pub async fn add_tag_to_target(
        &self,
        target_type: &str,
        target_id: &str,
        name: &str,
        color: &str,
    ) -> Result<TagView, sqlx::Error> {
        let target_type = normalize_tag_target_type(target_type);
        let target_id = target_id.trim();
        if target_type.is_empty() || target_id.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }

        let normalized_name = normalize_tag_name(name);
        if normalized_name.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }
        let normalized_color = normalize_tag_color(color);
        let existing = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                COUNT(it.id) AS target_count,
                t.created_at,
                t.updated_at
            FROM tags t
            LEFT JOIN item_tags it ON it.tag_id = t.id
            WHERE lower(t.name) = lower(?)
            GROUP BY t.id, t.name, t.color, t.created_at, t.updated_at
            "#,
        )
        .bind(&normalized_name)
        .fetch_optional(&self.pool)
        .await?;

        let tag = if let Some(existing) = existing {
            existing
        } else {
            let now = current_unix_ts();
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO tags (id, name, color, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&normalized_name)
            .bind(&normalized_color)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
            TagRow {
                id,
                name: normalized_name.clone(),
                color: normalized_color.clone(),
                target_count: 0,
                created_at: now,
                updated_at: now,
            }
        };

        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO item_tags (id, target_type, target_id, tag_id, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(target_type)
        .bind(target_id)
        .bind(&tag.id)
        .bind(now)
        .execute(&self.pool)
        .await?;

        sqlx::query("UPDATE tags SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(&tag.id)
            .execute(&self.pool)
            .await?;

        Ok(TagView {
            id: tag.id,
            name: tag.name,
            color: tag.color,
            target_count: tag.target_count.max(0) as usize,
            created_at: format_unix_ts(tag.created_at),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn remove_tag_from_target(
        &self,
        target_type: &str,
        target_id: &str,
        tag_id: &str,
    ) -> Result<(), sqlx::Error> {
        let target_type = normalize_tag_target_type(target_type);
        if target_type.is_empty() || target_id.trim().is_empty() || tag_id.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM item_tags WHERE target_type = ? AND target_id = ? AND tag_id = ?")
            .bind(target_type)
            .bind(target_id.trim())
            .bind(tag_id.trim())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_qa_session(&self, title: &str) -> Result<QaSessionView, sqlx::Error> {
        let now = current_unix_ts();
        let id = Uuid::new_v4().to_string();
        let normalized_title = normalize_qa_session_title(title);
        sqlx::query(
            r#"
            INSERT INTO qa_sessions (id, title, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&normalized_title)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(QaSessionView {
            id,
            title: normalized_title,
            message_count: 0,
            created_at: format_unix_ts(now),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn list_qa_sessions(&self, limit: i64) -> Result<Vec<QaSessionView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, QaSessionRow>(
            r#"
            SELECT
                s.id,
                s.title,
                COUNT(m.id) AS message_count,
                s.created_at,
                s.updated_at
            FROM qa_sessions s
            LEFT JOIN qa_messages m ON m.session_id = s.id
            GROUP BY s.id
            ORDER BY s.updated_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(qa_session_row_to_view).collect())
    }

    pub async fn list_qa_messages(
        &self,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<QaMessageView>, sqlx::Error> {
        if session_id.trim().is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, QaMessageRow>(
            r#"
            SELECT id, session_id, question, answer, state, sources_json, retrieval_json, model, error, created_at, updated_at
            FROM qa_messages
            WHERE session_id = ?
            ORDER BY created_at ASC
            LIMIT ?
            "#,
        )
        .bind(session_id.trim())
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(qa_message_row_to_view).collect())
    }

    pub async fn list_qa_messages_recent(
        &self,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<QaMessageView>, sqlx::Error> {
        if session_id.trim().is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, QaMessageRow>(
            r#"
            SELECT id, session_id, question, answer, state, sources_json, retrieval_json, model, error, created_at, updated_at
            FROM qa_messages
            WHERE session_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(session_id.trim())
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        let mut messages = rows
            .into_iter()
            .map(qa_message_row_to_view)
            .collect::<Vec<_>>();
        messages.reverse();
        Ok(messages)
    }

    pub async fn remove_qa_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        if session_id.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM qa_messages WHERE session_id = ?")
            .bind(session_id.trim())
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM qa_sessions WHERE id = ?")
            .bind(session_id.trim())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_qa_session_title(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<(), sqlx::Error> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Ok(());
        }

        let now = current_unix_ts();
        sqlx::query(
            r#"
            UPDATE qa_sessions
            SET title = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(normalize_qa_session_title(title))
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_qa_answer(
        &self,
        answer: &QaAnswerView,
        session_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.record_qa_history(answer).await?;
        if let Some(session_id) = session_id.map(str::trim).filter(|id| !id.is_empty()) {
            self.record_qa_message(session_id, answer).await?;
        }
        Ok(())
    }

    pub async fn record_qa_message(
        &self,
        session_id: &str,
        answer: &QaAnswerView,
    ) -> Result<(), sqlx::Error> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Ok(());
        }

        let now = current_unix_ts();
        let sources_json =
            serde_json::to_string(&answer.sources).unwrap_or_else(|_| "[]".to_string());
        let retrieval_json =
            serde_json::to_string(&answer.retrieval).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_messages
                (id, session_id, question, answer, state, sources_json, retrieval_json, model, error, created_at, updated_at)
            VALUES (
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                COALESCE((SELECT created_at FROM qa_messages WHERE id = ?), ?),
                ?
            )
            "#,
        )
        .bind(&answer.id)
        .bind(session_id)
        .bind(&answer.question)
        .bind(&answer.answer)
        .bind(&answer.state)
        .bind(sources_json)
        .bind(retrieval_json)
        .bind(&answer.model)
        .bind(answer.error.as_deref().unwrap_or(""))
        .bind(&answer.id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        sqlx::query("UPDATE qa_sessions SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
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

    pub async fn remove_favorite(&self, target: &str) -> Result<(), sqlx::Error> {
        if target.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM favorites WHERE target = ?")
            .bind(target)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_collections(&self) -> Result<Vec<CollectionView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT
                c.id,
                c.name,
                c.description,
                c.color,
                c.sort_order,
                COUNT(i.id) AS item_count,
                c.created_at,
                c.updated_at
            FROM collections c
            LEFT JOIN collection_items i ON i.collection_id = c.id
            GROUP BY c.id, c.name, c.description, c.color, c.sort_order, c.created_at, c.updated_at
            ORDER BY c.sort_order ASC, c.updated_at DESC, c.name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(collection_row_to_view).collect())
    }

    pub async fn create_collection(
        &self,
        name: &str,
        description: &str,
    ) -> Result<CollectionView, sqlx::Error> {
        let now = current_unix_ts();
        let sort_order = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM collections",
        )
        .fetch_one(&self.pool)
        .await?;
        let id = Uuid::new_v4().to_string();
        let normalized_name = normalize_collection_name(name);
        let normalized_description = normalize_collection_description(description);

        sqlx::query(
            r#"
            INSERT INTO collections
                (id, name, description, color, sort_order, created_at, updated_at)
            VALUES (?, ?, ?, '', ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&normalized_name)
        .bind(&normalized_description)
        .bind(sort_order)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(CollectionView {
            id,
            name: normalized_name,
            description: normalized_description,
            color: String::new(),
            sort_order,
            item_count: 0,
            created_at: format_unix_ts(now),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn update_collection(
        &self,
        collection_id: &str,
        patch: &CollectionPatchInput,
    ) -> Result<CollectionView, sqlx::Error> {
        let existing = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT
                c.id,
                c.name,
                c.description,
                c.color,
                c.sort_order,
                COUNT(i.id) AS item_count,
                c.created_at,
                c.updated_at
            FROM collections c
            LEFT JOIN collection_items i ON i.collection_id = c.id
            WHERE c.id = ?
            GROUP BY c.id, c.name, c.description, c.color, c.sort_order, c.created_at, c.updated_at
            LIMIT 1
            "#,
        )
        .bind(collection_id.trim())
        .fetch_optional(&self.pool)
        .await?;

        let Some(existing) = existing else {
            return Err(sqlx::Error::RowNotFound);
        };

        let next_name = patch
            .name
            .as_deref()
            .map(normalize_collection_name)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(existing.name.clone());
        let next_description = patch
            .description
            .as_deref()
            .map(normalize_collection_description)
            .unwrap_or(existing.description.clone());
        let next_color = patch
            .color
            .as_deref()
            .map(normalize_collection_color)
            .unwrap_or(existing.color.clone());
        let now = current_unix_ts();

        sqlx::query(
            r#"
            UPDATE collections
            SET name = ?, description = ?, color = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&next_name)
        .bind(&next_description)
        .bind(&next_color)
        .bind(now)
        .bind(collection_id.trim())
        .execute(&self.pool)
        .await?;

        Ok(CollectionView {
            id: existing.id,
            name: next_name,
            description: next_description,
            color: next_color,
            sort_order: existing.sort_order,
            item_count: existing.item_count.max(0) as usize,
            created_at: format_unix_ts(existing.created_at),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn delete_collection(&self, collection_id: &str) -> Result<(), sqlx::Error> {
        let collection_id = collection_id.trim();
        if collection_id.is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(collection_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionItemView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, CollectionItemRow>(
            r#"
            SELECT
                id,
                collection_id,
                item_type,
                document_id,
                chunk_id,
                qa_session_id,
                qa_message_id,
                title,
                path,
                title_path,
                snippet,
                note,
                source_meta_json,
                sort_order,
                created_at,
                updated_at
            FROM collection_items
            WHERE collection_id = ?
            ORDER BY sort_order ASC, updated_at DESC, title ASC
            "#,
        )
        .bind(collection_id.trim())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(collection_item_row_to_view).collect())
    }

    pub async fn add_collection_item(
        &self,
        input: &CollectionItemInput,
    ) -> Result<CollectionItemView, sqlx::Error> {
        let collection_id = input.collection_id.trim();
        if collection_id.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }

        let item_type = normalize_collection_item_type(&input.item_type);
        let document_id = input.document_id.clone().unwrap_or_default();
        let chunk_id = input.chunk_id.clone().unwrap_or_default();
        let qa_session_id = input.qa_session_id.clone().unwrap_or_default();
        let qa_message_id = input.qa_message_id.clone().unwrap_or_default();
        let title = input.title.trim().to_string();
        let path = input.path.clone().unwrap_or_default();
        let title_path = input.title_path.clone().unwrap_or_default();
        let snippet = input.snippet.clone().unwrap_or_default();
        let note = input.note.clone().unwrap_or_default();
        let source_meta_json = input
            .source_meta_json
            .clone()
            .unwrap_or_else(|| "{}".to_string());
        let now = current_unix_ts();

        let existing_id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT id
            FROM collection_items
            WHERE collection_id = ?
              AND item_type = ?
              AND document_id = ?
              AND chunk_id = ?
              AND qa_session_id = ?
              AND qa_message_id = ?
              AND path = ?
              AND title_path = ?
            LIMIT 1
            "#,
        )
        .bind(collection_id)
        .bind(&item_type)
        .bind(&document_id)
        .bind(&chunk_id)
        .bind(&qa_session_id)
        .bind(&qa_message_id)
        .bind(&path)
        .bind(&title_path)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(existing_id) = existing_id {
            sqlx::query(
                r#"
                UPDATE collection_items
                SET title = ?, snippet = ?, note = ?, source_meta_json = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(&title)
            .bind(&snippet)
            .bind(&note)
            .bind(&source_meta_json)
            .bind(now)
            .bind(&existing_id)
            .execute(&self.pool)
            .await?;

            let row = sqlx::query_as::<_, CollectionItemRow>(
                r#"
                SELECT id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at
                FROM collection_items
                WHERE id = ?
                "#,
            )
            .bind(&existing_id)
            .fetch_one(&self.pool)
            .await?;
            return Ok(collection_item_row_to_view(row));
        }

        let sort_order = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM collection_items WHERE collection_id = ?",
        )
        .bind(collection_id)
        .fetch_one(&self.pool)
        .await?;
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO collection_items
                (id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(collection_id)
        .bind(&item_type)
        .bind(&document_id)
        .bind(&chunk_id)
        .bind(&qa_session_id)
        .bind(&qa_message_id)
        .bind(&title)
        .bind(&path)
        .bind(&title_path)
        .bind(&snippet)
        .bind(&note)
        .bind(&source_meta_json)
        .bind(sort_order)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, CollectionItemRow>(
            r#"
            SELECT id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at
            FROM collection_items
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;

        Ok(collection_item_row_to_view(row))
    }

    pub async fn update_collection_item_note(
        &self,
        item_id: &str,
        note: &str,
    ) -> Result<CollectionItemView, sqlx::Error> {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }

        let now = current_unix_ts();
        sqlx::query(
            r#"
            UPDATE collection_items
            SET note = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(note.trim())
        .bind(now)
        .bind(item_id)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, CollectionItemRow>(
            r#"
            SELECT id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at
            FROM collection_items
            WHERE id = ?
            "#,
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(collection_item_row_to_view(row))
    }

    pub async fn remove_collection_item(&self, item_id: &str) -> Result<(), sqlx::Error> {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM collection_items WHERE id = ?")
            .bind(item_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn export_collection_markdown(
        &self,
        collection_id: &str,
    ) -> Result<String, sqlx::Error> {
        let collection = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT
                c.id,
                c.name,
                c.description,
                c.color,
                c.sort_order,
                COUNT(i.id) AS item_count,
                c.created_at,
                c.updated_at
            FROM collections c
            LEFT JOIN collection_items i ON i.collection_id = c.id
            WHERE c.id = ?
            GROUP BY c.id, c.name, c.description, c.color, c.sort_order, c.created_at, c.updated_at
            LIMIT 1
            "#,
        )
        .bind(collection_id.trim())
        .fetch_optional(&self.pool)
        .await?;

        let Some(collection) = collection else {
            return Err(sqlx::Error::RowNotFound);
        };

        let items = self.list_collection_items(collection_id).await?;
        let mut markdown = String::new();
        markdown.push_str(&format!("# {}\n\n", collection.name));
        if !collection.description.trim().is_empty() {
            markdown.push_str(&format!("> {}\n\n", collection.description.trim()));
        }

        for (index, item) in items.iter().enumerate() {
            markdown.push_str(&format!("## {}. {}\n\n", index + 1, item.title.trim()));
            markdown.push_str(&format!("- 类型：{}\n", item.item_type));
            if !item.path.trim().is_empty() {
                markdown.push_str(&format!("- 路径：{}\n", item.path.trim()));
            }
            if !item.title_path.trim().is_empty() {
                markdown.push_str(&format!("- 定位：{}\n", item.title_path.trim()));
            }
            if !item.snippet.trim().is_empty() {
                markdown.push_str("\n摘录：\n\n");
                markdown.push_str(&format!("> {}\n", item.snippet.trim()));
            }
            if !item.note.trim().is_empty() {
                markdown.push_str("\n备注：\n\n");
                markdown.push_str(&item.note.trim());
                markdown.push('\n');
            }
            markdown.push('\n');
        }

        Ok(markdown)
    }

    pub async fn default_embedding_model_available(&self) -> Result<bool, sqlx::Error> {
        #[derive(Debug, sqlx::FromRow)]
        struct DefaultEmbeddingModelRow {
            available: i64,
        }

        let row = sqlx::query_as::<_, DefaultEmbeddingModelRow>(
            r#"
            SELECT available
            FROM embedding_models
            WHERE is_default = 1
            ORDER BY updated_at DESC, name ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|item| item.available != 0).unwrap_or(false))
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
        .bind(if heading.trim().is_empty() {
            file_name
        } else {
            heading
        })
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
        "[DocMind] preview image block document={} raw_asset_path={} resolved_asset_path={} exists={}",
        document_path, raw_asset_path, resolved_asset_path, exists
    );
    }

    fn build_preview_block(
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

    pub(crate) async fn search_documents_debug(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchDebugData, sqlx::Error> {
        self.build_search_results(query, limit).await
    }

    async fn build_search_results(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchDebugData, sqlx::Error> {
        let settings = self.get_index_settings().await?;
        let semantic_enabled = settings.semantic_search_enabled;
        let semantic_weight = settings.semantic_weight.clamp(0.0, 1.0);
        let semantic_threshold = settings.semantic_threshold.clamp(-1.0, 1.0);
        let rewritten_terms = rewrite_query_terms(query);
        let rewritten_query = rewrite_search_text(query);
        let semantic_model_available = self
            .default_embedding_model_available()
            .await
            .unwrap_or(false);
        let history_terms = if settings.prefer_history_enabled {
            self.derive_history_terms(&rewritten_terms)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let history_rewrite_applied = !history_terms.is_empty();
        let mut expanded_terms = rewritten_terms.clone();
        expanded_terms.extend(history_terms.iter().cloned());
        let expanded_query = expanded_terms.join(" ");

        let keyword_hits = self
            .search_index
            .search(&expanded_query, limit.max(1))
            .map_err(sqlx::Error::Protocol)?;
        let recent_documents = if settings.prefer_recent_enabled {
            self.list_recent_documents(50).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let favorites = if settings.prefer_favorites_enabled {
            self.list_favorites(200).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let recent_document_map = recent_documents
            .into_iter()
            .map(|item| (item.path, item.open_count))
            .collect::<std::collections::HashMap<_, _>>();
        let favorite_targets = favorites
            .into_iter()
            .map(|item| item.target)
            .collect::<std::collections::HashSet<_>>();

        let semantic_limit = limit.max(1).saturating_mul(3).max(limit.max(1));
        let semantic_result = if semantic_enabled {
            semantic_store::semantic_search_hits(self, query, semantic_limit).await
        } else {
            Ok(Vec::new())
        };
        let (semantic_candidates, semantic_fallback, semantic_fallback_reason) =
            match semantic_result {
                Ok(hits) => (hits, false, String::new()),
                Err(error) => (Vec::new(), true, error),
            };
        let semantic_fallback_reason_text = if semantic_fallback_reason.is_empty() {
            if !semantic_enabled {
                "语义检索已关闭".to_string()
            } else if !semantic_model_available {
                "语义模型不可用".to_string()
            } else {
                String::new()
            }
        } else {
            semantic_fallback_reason.clone()
        };
        let semantic_candidate_count = semantic_candidates.len();
        let semantic_hits: Vec<_> = semantic_candidates
            .into_iter()
            .filter(|hit| hit.score >= semantic_threshold)
            .collect();
        let semantic_filtered_count = semantic_candidate_count.saturating_sub(semantic_hits.len());

        if keyword_hits.is_empty() && semantic_hits.is_empty() {
            return Ok(SearchDebugData {
                hits: Vec::new(),
                keyword_hit_count: 0,
                semantic_hit_count: 0,
                semantic_candidate_count,
                semantic_filtered_count,
                semantic_enabled,
                semantic_weight,
                semantic_threshold,
                rewritten_terms,
                rewritten_query,
                history_terms,
                history_rewrite_applied,
                expanded_query,
                semantic_fallback: semantic_fallback || !semantic_model_available,
                semantic_fallback_reason: semantic_fallback_reason_text.clone(),
                search_mode: if semantic_enabled {
                    "hybrid".to_string()
                } else {
                    "fulltext".to_string()
                },
            });
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
        let mut preview_blocks_by_chunk_id =
            self.fetch_preview_blocks_for_search_rows(&rows).await?;
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
                let title_score = if row.heading.trim().is_empty() {
                    0.0
                } else if contains_all_terms(&row.heading, &normalized_terms) {
                    0.32
                } else if contains_any_term(&row.heading, &normalized_terms) {
                    0.18
                } else {
                    0.0
                };
                let filename_score = if row.file_name.trim().is_empty() {
                    0.0
                } else if contains_all_terms(&row.file_name, &normalized_terms) {
                    0.45
                } else if contains_any_term(&row.file_name, &normalized_terms) {
                    0.25
                } else {
                    0.0
                };
                let is_favorite = favorite_targets.contains(&favorite_result_target(
                    &row.path,
                    &row.heading,
                    row.paragraph.map(|value| value as u32),
                    row.page.map(|value| value as u32),
                ));
                let recent_open_count = recent_document_map.get(&row.path).copied().unwrap_or(0);
                let preference_score =
                    rerank_bonus(is_favorite, recent_open_count, history_rewrite_applied);
                let (
                    snippet,
                    highlight_spans,
                    snippet_window_start,
                    snippet_window_end,
                    snippet_source_len,
                ) = build_search_snippet(&row.snippet, &normalized_terms, 220);
                let (matched_field, match_origin) = if keyword_score > 0.0 {
                    matched_field_and_origin(&row, &normalized_terms)
                } else if semantic_score > 0.0 {
                    ("semantic".to_string(), "语义命中".to_string())
                } else {
                    matched_field_and_origin(&row, &normalized_terms)
                };
                let rank_reason = search_rank_reason(
                    &row,
                    &normalized_terms,
                    &match_origin,
                    keyword_score,
                    semantic_score,
                    semantic_enabled,
                    is_favorite,
                    recent_open_count,
                    history_rewrite_applied,
                    title_score,
                    filename_score,
                    preference_score,
                    row.modified_at,
                    now,
                );
                let base_score = if keyword_score > 0.0 {
                    keyword_score + semantic_score.max(0.0) * semantic_weight
                } else {
                    semantic_score.max(0.0)
                };
                let chunk_weight = row.score.clamp(0.25, 1.0);
                let raw_score = boosted_search_score(
                    base_score * chunk_weight,
                    &match_origin,
                    row.modified_at,
                    now,
                );
                let weighted_title_score = title_score * settings.title_weight;
                let weighted_filename_score = filename_score * settings.filename_weight;
                let weighted_preference_score = preference_score * settings.preference_weight;
                let final_score = raw_score
                    + weighted_preference_score
                    + weighted_title_score
                    + weighted_filename_score;
                let mut rank_reason = rank_reason;
                rank_reason.base_score = base_score;
                rank_reason.raw_score = raw_score;
                rank_reason.title_score = weighted_title_score;
                rank_reason.filename_score = weighted_filename_score;
                rank_reason.preference_score = weighted_preference_score;
                results.push(SearchResultCandidate {
                    result: SearchResultView {
                        id: row.id,
                        file_name: row.file_name,
                        path: row.path,
                        ext: row.ext,
                        heading: row.heading.clone(),
                        title_path: row.heading,
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
                        score: final_score,
                        rank_reason,
                        preview_blocks: preview_blocks_by_chunk_id
                            .remove(&chunk_id)
                            .unwrap_or_default(),
                    },
                    raw_score,
                    final_score,
                });
            }
        }

        let mut original_rank_map = std::collections::HashMap::<String, usize>::new();
        let mut original_order = results.clone();
        original_order.sort_by(|left, right| {
            right
                .raw_score
                .partial_cmp(&left.raw_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (index, candidate) in original_order.iter().enumerate() {
            original_rank_map.insert(candidate.result.id.clone(), index + 1);
        }

        results.sort_by(|left, right| {
            right
                .final_score
                .partial_cmp(&left.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut final_results = Vec::new();
        for (index, candidate) in results.into_iter().enumerate() {
            let mut result = candidate.result;
            let original_rank = original_rank_map
                .get(&result.id)
                .copied()
                .unwrap_or(index + 1);
            let final_rank = index + 1;
            result.rank_reason.original_rank = original_rank;
            result.rank_reason.final_rank = final_rank;
            result.rank_reason.rank_delta = original_rank as isize - final_rank as isize;
            final_results.push(result);
        }
        final_results.truncate(limit.max(1));

        Ok(SearchDebugData {
            hits: final_results,
            keyword_hit_count: keyword_hits.len(),
            semantic_hit_count: semantic_hits.len(),
            semantic_candidate_count,
            semantic_filtered_count,
            semantic_enabled,
            semantic_weight,
            semantic_threshold,
            rewritten_terms,
            rewritten_query,
            history_terms,
            history_rewrite_applied,
            expanded_query,
            semantic_fallback: semantic_fallback || !semantic_model_available,
            semantic_fallback_reason: semantic_fallback_reason_text,
            search_mode: if semantic_enabled {
                "hybrid".to_string()
            } else {
                "fulltext".to_string()
            },
        })
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

    pub async fn quick_look_file(&self, app: &tauri::AppHandle, path: &str) -> Result<(), String> {
        file_ops::quick_look_file_path(app, path)
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

    pub(crate) async fn clear_directory_documents(
        &self,
        dir_path: &str,
    ) -> Result<(), sqlx::Error> {
        self.search_index
            .delete_directory(dir_path)
            .map_err(sqlx::Error::Protocol)?;
        if is_virtual_directory(dir_path) {
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

    pub(crate) async fn document_id_by_path(
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

    pub(crate) async fn store_document(
        &self,
        document: &ExtractedDocument,
        chunks: &[ChunkRecord],
        blocks: &[crate::docmind::parser::types::ParsedBlock],
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

    async fn last_index_run_summary(
        &self,
    ) -> Result<Option<crate::docmind::models::IndexRunSummaryView>, sqlx::Error> {
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

    pub(crate) async fn load_index_checkpoint(
        &self,
    ) -> Result<Option<IndexCheckpointRow>, sqlx::Error> {
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
        warning: Option<&str>,
        pause_requested: bool,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        let existing_started_at = sqlx::query_as::<_, CurrentTaskRow>(
            r#"
            SELECT label, details, state, current_dir, current_file, started_at, progress, scanned, total, succeeded, failed, updated, skipped, deleted, warning, pause_requested
            FROM current_task
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|row| {
            // 修复：运行中/暂停中的任务更新时间要沿用首次启动时间，避免耗时跳变。
            if matches!(row.state.as_str(), "running" | "paused") {
                row.started_at.max(0)
            } else {
                0
            }
        })
        .unwrap_or(0);
        let started_at = if existing_started_at > 0 {
            existing_started_at
        } else {
            now
        };
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO current_task
                (id, label, details, state, current_dir, current_file, started_at, progress, scanned, total, succeeded, failed, updated, skipped, deleted, warning, pause_requested)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(label)
        .bind(details)
        .bind(state)
        .bind(current_dir)
        .bind(current_file)
        .bind(started_at)
        .bind(progress as i64)
        .bind(scanned as i64)
        .bind(total as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
        .bind(updated as i64)
        .bind(skipped as i64)
        .bind(deleted as i64)
        .bind(warning)
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

    pub async fn save_network_proxy_settings(
        &self,
        settings: &NetworkProxySettings,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO network_proxy_settings
                (id, enabled, proxy_url, updated_at)
            VALUES (1, ?, ?, ?)
            "#,
        )
        .bind(settings.enabled as i64)
        .bind(settings.proxy_url.trim())
        .bind(now)
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

        sqlx::query("DELETE FROM index_checkpoint")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM current_task")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM index_run_summary")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM failed_files")
            .execute(&self.pool)
            .await?;
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
        sqlx::query("DELETE FROM document_blocks")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM chunks")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM documents")
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
        let (docs, chunks) = if is_virtual_directory(path) {
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
            (docs, chunks)
        } else {
            let prefix = format!("{path}/%");
            let docs = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM documents
                WHERE path = ? OR path LIKE ?
                "#,
            )
            .bind(path)
            .bind(&prefix)
            .fetch_one(&self.pool)
            .await? as usize;

            let chunks = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM chunks c
                INNER JOIN documents d ON d.id = c.document_id
                WHERE d.path = ? OR d.path LIKE ?
                "#,
            )
            .bind(path)
            .bind(&prefix)
            .fetch_one(&self.pool)
            .await? as usize;
            (docs, chunks)
        };

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
                block_indexes_json TEXT NOT NULL DEFAULT '[]',
                FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS document_blocks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                block_index INTEGER NOT NULL,
                block_type TEXT NOT NULL,
                text TEXT NOT NULL,
                heading TEXT NOT NULL DEFAULT '',
                level INTEGER,
                page INTEGER,
                language TEXT NOT NULL DEFAULT '',
                markdown TEXT NOT NULL DEFAULT '',
                html TEXT NOT NULL DEFAULT '',
                asset_path TEXT NOT NULL DEFAULT '',
                alt_text TEXT NOT NULL DEFAULT '',
                caption TEXT NOT NULL DEFAULT '',
                ocr_text TEXT NOT NULL DEFAULT '',
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
                started_at INTEGER NOT NULL DEFAULT 0,
                progress INTEGER NOT NULL,
                scanned INTEGER NOT NULL,
                total INTEGER NOT NULL,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0,
                warning TEXT NOT NULL DEFAULT '',
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
                max_file_size_mb INTEGER NOT NULL,
                semantic_search_enabled INTEGER NOT NULL DEFAULT 1,
                semantic_weight REAL NOT NULL DEFAULT 0.25,
                semantic_threshold REAL NOT NULL DEFAULT 0.2,
                title_weight REAL NOT NULL DEFAULT 1,
                filename_weight REAL NOT NULL DEFAULT 1,
                preference_weight REAL NOT NULL DEFAULT 1,
                prefer_favorites_enabled INTEGER NOT NULL DEFAULT 1,
                prefer_recent_enabled INTEGER NOT NULL DEFAULT 1,
                prefer_history_enabled INTEGER NOT NULL DEFAULT 1
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
            CREATE TABLE IF NOT EXISTS recent_views (
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                title TEXT NOT NULL,
                path TEXT NOT NULL DEFAULT '',
                viewed_at INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (target_type, target_id)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS item_tags (
                id TEXT PRIMARY KEY,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE,
                UNIQUE(target_type, target_id, tag_id)
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
            r#"
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                color TEXT NOT NULL DEFAULT '',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS collection_items (
                id TEXT PRIMARY KEY,
                collection_id TEXT NOT NULL,
                item_type TEXT NOT NULL,
                document_id TEXT NOT NULL DEFAULT '',
                chunk_id TEXT NOT NULL DEFAULT '',
                qa_session_id TEXT NOT NULL DEFAULT '',
                qa_message_id TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                path TEXT NOT NULL DEFAULT '',
                title_path TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                note TEXT NOT NULL DEFAULT '',
                source_meta_json TEXT NOT NULL DEFAULT '{}',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                provider TEXT NOT NULL DEFAULT 'openai_compatible',
                base_url TEXT NOT NULL DEFAULT '',
                api_key TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                temperature REAL NOT NULL DEFAULT 0.2,
                max_output_tokens INTEGER NOT NULL DEFAULT 600,
                context_chunk_limit INTEGER NOT NULL DEFAULT 8,
                context_token_budget INTEGER NOT NULL DEFAULT 6000,
                min_evidence_count INTEGER NOT NULL DEFAULT 2,
                min_retrieval_score REAL NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS network_proxy_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                proxy_url TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_model_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT NOT NULL DEFAULT '',
                api_key TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_history (
                id TEXT PRIMARY KEY,
                question TEXT NOT NULL,
                answer TEXT NOT NULL,
                state TEXT NOT NULL,
                sources_json TEXT NOT NULL DEFAULT '[]',
                retrieval_json TEXT NOT NULL DEFAULT '{}',
                model TEXT NOT NULL DEFAULT '',
                error TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                question TEXT NOT NULL,
                answer TEXT NOT NULL,
                state TEXT NOT NULL,
                sources_json TEXT NOT NULL DEFAULT '[]',
                retrieval_json TEXT NOT NULL DEFAULT '{}',
                model TEXT NOT NULL DEFAULT '',
                error TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(session_id) REFERENCES qa_sessions(id) ON DELETE CASCADE
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
                semantic_search_enabled: true,
                semantic_weight: 0.25,
                semantic_threshold: 0.2,
                title_weight: 1.0,
                filename_weight: 1.0,
                preference_weight: 1.0,
                prefer_favorites_enabled: true,
                prefer_recent_enabled: true,
                prefer_history_enabled: true,
            };
            self.save_index_settings(&defaults).await?;
        }
        Ok(())
    }

    async fn ensure_qa_settings_row(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM qa_settings").await?;
        if count == 0 {
            self.save_qa_settings(&default_qa_settings()).await?;
        }
        Ok(())
    }

    async fn ensure_network_proxy_settings_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM network_proxy_settings").await?;
        if count == 0 {
            self.save_network_proxy_settings(&default_network_proxy_settings())
                .await?;
        }
        Ok(())
    }

    async fn ensure_collections_seed(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM collections").await?;
        if count == 0 {
            self.create_collection("默认主题集合", "").await?;
        }
        Ok(())
    }

    async fn ensure_qa_model_profiles_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM qa_model_profiles").await?;
        if count == 0 {
            let settings = self
                .get_qa_settings()
                .await
                .unwrap_or_else(|_| default_qa_settings());
            let now = current_unix_ts();
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO qa_model_profiles
                    (id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?)
                "#,
            )
            .bind(&id)
            .bind("默认连接")
            .bind(settings.provider)
            .bind(settings.base_url)
            .bind(settings.api_key)
            .bind(settings.model)
            .bind(settings.enabled as i64)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn ensure_embedding_models_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM embedding_models").await?;
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
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM vector_index_meta").await?;
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
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN current_dir TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("current_file") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN current_file TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("started_at") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN started_at INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("succeeded") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN succeeded INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("failed") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN failed INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("updated") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN updated INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("skipped") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN skipped INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("deleted") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("warning") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN warning TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("state") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN state TEXT NOT NULL DEFAULT 'idle'");
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
            alter_statements
                .push("ALTER TABLE failed_files ADD COLUMN code TEXT NOT NULL DEFAULT 'unknown'");
        }
        if !columns.contains("retry_count") {
            alter_statements
                .push("ALTER TABLE failed_files ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0");
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
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM index_run_summary").await?;
        if count == 0 {
            self.save_index_run_summary(0, 0, 0, 0, 0, 0, 0).await?;
        }
        Ok(())
    }

    async fn ensure_index_settings_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(index_settings)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("semantic_search_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN semantic_search_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("semantic_weight") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN semantic_weight REAL NOT NULL DEFAULT 0.25",
            );
        }
        if !columns.contains("semantic_threshold") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN semantic_threshold REAL NOT NULL DEFAULT 0.2",
            );
        }
        if !columns.contains("title_weight") {
            alter_statements
                .push("ALTER TABLE index_settings ADD COLUMN title_weight REAL NOT NULL DEFAULT 1");
        }
        if !columns.contains("filename_weight") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN filename_weight REAL NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("preference_weight") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN preference_weight REAL NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("prefer_favorites_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN prefer_favorites_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("prefer_recent_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN prefer_recent_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("prefer_history_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN prefer_history_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
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
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN dir_path TEXT NOT NULL DEFAULT ''");
            altered = true;
        }
        if !columns.contains("file_size") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0");
            altered = true;
        }
        if !columns.contains("modified_at") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN modified_at INTEGER NOT NULL DEFAULT 0");
            altered = true;
        }
        if !columns.contains("content_hash") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN content_hash TEXT NOT NULL DEFAULT ''");
            altered = true;
        }
        if !columns.contains("content") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN content TEXT NOT NULL DEFAULT ''");
            altered = true;
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(altered)
    }

    async fn ensure_chunks_block_indexes_column(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(chunks)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        if !columns.contains("block_indexes_json") {
            sqlx::query(
                r#"
                ALTER TABLE chunks ADD COLUMN block_indexes_json TEXT NOT NULL DEFAULT '[]'
                "#,
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn ensure_document_blocks_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(document_blocks)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("asset_path") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN asset_path TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("alt_text") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN alt_text TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("caption") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN caption TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("language") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN language TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("ocr_text") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN ocr_text TEXT NOT NULL DEFAULT ''");
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
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
            SELECT label, details, state, current_dir, current_file, started_at, progress, scanned, total, succeeded, failed, updated, skipped, deleted, warning, pause_requested
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
            started_at: row.started_at,
            progress: row.progress as u8,
            scanned: row.scanned as usize,
            total: row.total as usize,
            succeeded: row.succeeded as usize,
            failed: row.failed as usize,
            updated: row.updated as usize,
            skipped: row.skipped as usize,
            deleted: row.deleted as usize,
            warning: row.warning.and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),
            pause_requested: row.pause_requested != 0,
        }))
    }

    async fn fetch_chunks_by_ids(
        &self,
        chunk_ids: &[String],
    ) -> Result<Vec<SearchRow>, sqlx::Error> {
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
                c.document_id,
                d.file_name,
                d.path,
                d.ext,
                c.heading,
                c.snippet,
                c.paragraph,
                c.page,
                d.modified,
                d.modified_at,
                c.score,
                c.block_indexes_json
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

    async fn fetch_preview_blocks_for_search_rows(
        &self,
        rows: &[SearchRow],
    ) -> Result<std::collections::HashMap<String, Vec<PreviewBlockView>>, sqlx::Error> {
        if rows.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let mut document_ids = rows
            .iter()
            .map(|row| row.document_id.clone())
            .collect::<Vec<_>>();
        document_ids.sort();
        document_ids.dedup();

        let placeholders = std::iter::repeat("?")
            .take(document_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"
            SELECT document_id, block_index, block_type, text, heading, level, page, language, markdown, html, asset_path, alt_text, caption, ocr_text
            FROM document_blocks
            WHERE document_id IN ({})
            ORDER BY document_id, block_index
            "#,
            placeholders
        );

        let mut query_builder = sqlx::query_as::<_, BlockWithDocumentRow>(&sql);
        for document_id in &document_ids {
            query_builder = query_builder.bind(document_id);
        }

        let block_rows = query_builder.fetch_all(&self.pool).await?;
        let mut blocks_by_document = std::collections::HashMap::<
            String,
            std::collections::HashMap<i64, BlockWithDocumentRow>,
        >::new();
        for block in block_rows {
            blocks_by_document
                .entry(block.document_id.clone())
                .or_default()
                .insert(block.block_index, block);
        }

        let mut result = std::collections::HashMap::<String, Vec<PreviewBlockView>>::new();
        for row in rows {
            let block_indexes: Vec<usize> =
                serde_json::from_str(&row.block_indexes_json).unwrap_or_default();
            let Some(blocks_by_index) = blocks_by_document.get(&row.document_id) else {
                result.insert(row.id.clone(), Vec::new());
                continue;
            };

            let preview_blocks = block_indexes
                .iter()
                .filter_map(|index| {
                    blocks_by_index.get(&(*index as i64)).map(|block| {
                        Self::build_preview_block(
                            &row.path,
                            block.block_index,
                            &block.block_type,
                            &block.text,
                            &block.heading,
                            block.level,
                            block.page,
                            &block.language,
                            &block.markdown,
                            &block.html,
                            &block.asset_path,
                            &block.alt_text,
                            &block.caption,
                            &block.ocr_text,
                        )
                    })
                })
                .collect::<Vec<_>>();
            result.insert(row.id.clone(), preview_blocks);
        }

        Ok(result)
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

fn default_qa_settings() -> QaSettings {
    QaSettings {
        enabled: false,
        provider: "openai_compatible".to_string(),
        base_url: String::new(),
        api_key: String::new(),
        model: String::new(),
        temperature: 0.2,
        max_output_tokens: 600,
        context_chunk_limit: 8,
        context_token_budget: 6000,
        min_evidence_count: 1,
        min_retrieval_score: 0.0,
    }
}

fn default_network_proxy_settings() -> NetworkProxySettings {
    NetworkProxySettings {
        enabled: false,
        proxy_url: String::new(),
    }
}

fn qa_history_row_to_view(row: QaHistoryRow) -> QaHistoryView {
    let sources = serde_json::from_str::<Vec<QaSourceView>>(&row.sources_json).unwrap_or_default();
    let retrieval =
        serde_json::from_str::<QaRetrievalView>(&row.retrieval_json).unwrap_or(QaRetrievalView {
            search_mode: String::new(),
            candidate_count: 0,
            selected_count: 0,
            semantic_enabled: false,
            semantic_fallback: false,
            semantic_fallback_reason: String::new(),
        });

    QaHistoryView {
        id: row.id,
        question: row.question,
        answer: row.answer,
        state: row.state,
        sources,
        retrieval,
        model: row.model,
        created_at: format_unix_ts(row.created_at),
        error: {
            let trimmed = row.error.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
    }
}

fn qa_session_row_to_view(row: QaSessionRow) -> QaSessionView {
    QaSessionView {
        id: row.id,
        title: row.title,
        message_count: row.message_count.max(0) as usize,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn qa_message_row_to_view(row: QaMessageRow) -> QaMessageView {
    let sources = serde_json::from_str::<Vec<QaSourceView>>(&row.sources_json).unwrap_or_default();
    let retrieval =
        serde_json::from_str::<QaRetrievalView>(&row.retrieval_json).unwrap_or(QaRetrievalView {
            search_mode: String::new(),
            candidate_count: 0,
            selected_count: 0,
            semantic_enabled: false,
            semantic_fallback: false,
            semantic_fallback_reason: String::new(),
        });

    QaMessageView {
        id: row.id,
        session_id: row.session_id,
        question: row.question,
        answer: row.answer,
        state: row.state,
        sources,
        retrieval,
        model: row.model,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
        error: {
            let trimmed = row.error.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
    }
}

fn qa_model_profile_row_to_view(row: QaModelProfileRow) -> QaModelProfileView {
    QaModelProfileView {
        id: row.id,
        name: row.name,
        provider: row.provider,
        base_url: row.base_url,
        api_key: row.api_key,
        model: row.model,
        enabled: row.enabled != 0,
        is_default: row.is_default != 0,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn normalize_qa_session_title(title: &str) -> String {
    let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "新问答会话".to_string();
    }
    normalized.chars().take(40).collect()
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

    (
        snippet,
        adjusted_spans,
        snippet_start,
        snippet_end,
        total_chars,
    )
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

fn rerank_bonus(is_favorite: bool, recent_open_count: usize, history_expanded: bool) -> f32 {
    let mut bonus = 0.0;

    if is_favorite {
        bonus += 0.12;
    }

    if recent_open_count > 0 {
        bonus += 0.04;
        if recent_open_count >= 3 {
            bonus += 0.02;
        }
    }

    if history_expanded {
        bonus += 0.03;
    }

    bonus
}

fn search_rank_reason(
    row: &SearchRow,
    terms: &[String],
    match_origin: &str,
    keyword_score: f32,
    semantic_score: f32,
    semantic_enabled: bool,
    is_favorite: bool,
    recent_open_count: usize,
    history_expanded: bool,
    title_score: f32,
    filename_score: f32,
    preference_score: f32,
    modified_at: i64,
    now: i64,
) -> crate::docmind::models::SearchRankReasonView {
    let mut boosts = vec![match_origin.to_string()];

    if contains_all_terms(&row.file_name, terms) {
        boosts.push("文件名完整命中".to_string());
    } else if contains_all_terms(&row.heading, terms) {
        boosts.push("标题完整命中".to_string());
    } else if contains_all_terms(&row.path, terms) {
        boosts.push("路径完整命中".to_string());
    }

    if semantic_enabled {
        if keyword_score > 0.0 && semantic_score > 0.0 {
            boosts.push("全文+语义共同命中".to_string());
        } else if semantic_score > 0.0 {
            boosts.push("语义召回".to_string());
        }
    }

    if is_favorite {
        boosts.push("已收藏优先".to_string());
    }

    if recent_open_count > 0 {
        if recent_open_count >= 3 {
            boosts.push(format!("最近打开 {recent_open_count} 次"));
        } else {
            boosts.push("最近打开".to_string());
        }
    }

    if history_expanded {
        boosts.push("历史扩展".to_string());
    }

    if modified_at > 0 && now > modified_at {
        let age_days = ((now - modified_at) as f32 / 86_400.0).min(3_650.0);
        if age_days < 30.0 {
            boosts.push("最近更新".to_string());
        }
    }

    let boosts = dedupe_reason_parts(boosts);
    crate::docmind::models::SearchRankReasonView {
        summary: boosts.join(" · "),
        match_origin: match_origin.to_string(),
        boosts,
        keyword_hit: keyword_score > 0.0,
        semantic_hit: semantic_score > 0.0,
        favorite_boost: is_favorite,
        recent_open_count,
        history_expanded,
        keyword_score: keyword_score.max(0.0),
        semantic_score: semantic_score.max(0.0),
        title_score,
        filename_score,
        preference_score,
        base_score: 0.0,
        raw_score: 0.0,
        original_rank: 0,
        final_rank: 0,
        rank_delta: 0,
    }
}

fn dedupe_reason_parts(parts: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for part in parts {
        if part.trim().is_empty() {
            continue;
        }
        if !deduped.iter().any(|existing| existing == &part) {
            deduped.push(part);
        }
    }
    deduped
}

fn contains_all_terms(text: &str, terms: &[String]) -> bool {
    let lower = text.to_lowercase();
    let filtered_terms: Vec<String> = terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();

    !filtered_terms.is_empty() && filtered_terms.iter().all(|term| lower.contains(term))
}

fn contains_any_term(text: &str, terms: &[String]) -> bool {
    let lower = text.to_lowercase();
    let filtered_terms: Vec<String> = terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();

    !filtered_terms.is_empty() && filtered_terms.iter().any(|term| lower.contains(term))
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
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
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

fn normalize_collection_name(name: &str) -> String {
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "新主题集合".to_string();
    }

    normalized.chars().take(40).collect()
}

fn normalize_collection_description(description: &str) -> String {
    description.trim().chars().take(200).collect()
}

fn normalize_collection_color(color: &str) -> String {
    color.trim().chars().take(32).collect()
}

fn normalize_collection_item_type(item_type: &str) -> String {
    match item_type.trim().to_lowercase().as_str() {
        "document" => "document".to_string(),
        "chunk" => "chunk".to_string(),
        "search" => "search".to_string(),
        "qa_source" => "qa_source".to_string(),
        other if other.is_empty() => "chunk".to_string(),
        _ => "chunk".to_string(),
    }
}

fn normalize_recent_view_target_type(target_type: &str) -> String {
    match target_type.trim().to_lowercase().as_str() {
        "document" => "document".to_string(),
        "chunk" => "chunk".to_string(),
        "collection" => "collection".to_string(),
        "qa_session" => "qa_session".to_string(),
        _ => String::new(),
    }
}

fn normalize_tag_target_type(target_type: &str) -> String {
    match target_type.trim().to_lowercase().as_str() {
        "document" => "document".to_string(),
        "chunk" => "chunk".to_string(),
        "collection" => "collection".to_string(),
        "collection_item" => "collection_item".to_string(),
        _ => String::new(),
    }
}

fn normalize_tag_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(40)
        .collect()
}

fn normalize_tag_color(color: &str) -> String {
    color.trim().chars().take(32).collect()
}

fn collection_item_target_signature(item: &CollectionItemInput) -> String {
    [
        normalize_collection_item_type(&item.item_type),
        item.document_id.clone().unwrap_or_default(),
        item.chunk_id.clone().unwrap_or_default(),
        item.qa_session_id.clone().unwrap_or_default(),
        item.qa_message_id.clone().unwrap_or_default(),
        item.path.clone().unwrap_or_default(),
        item.title_path.clone().unwrap_or_default(),
    ]
    .join("|")
}

fn collection_row_to_view(row: CollectionRow) -> CollectionView {
    CollectionView {
        id: row.id,
        name: row.name,
        description: row.description,
        color: row.color,
        sort_order: row.sort_order,
        item_count: row.item_count.max(0) as usize,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn tag_row_to_view(row: TagRow) -> TagView {
    TagView {
        id: row.id,
        name: row.name,
        color: row.color,
        target_count: row.target_count.max(0) as usize,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn collection_item_row_to_view(row: CollectionItemRow) -> CollectionItemView {
    CollectionItemView {
        id: row.id,
        collection_id: row.collection_id,
        item_type: row.item_type,
        document_id: row.document_id,
        chunk_id: row.chunk_id,
        qa_session_id: row.qa_session_id,
        qa_message_id: row.qa_message_id,
        title: row.title,
        path: row.path,
        title_path: row.title_path,
        snippet: row.snippet,
        note: row.note,
        source_meta_json: row.source_meta_json,
        sort_order: row.sort_order,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn database_path() -> PathBuf {
    let base = data_dir().unwrap_or_else(|| PathBuf::from("."));
    #[cfg(debug_assertions)]
    {
        return base.join("DocMindDev").join("docmind.sqlite");
    }

    #[cfg(not(debug_assertions))]
    {
        base.join("DocMind").join("docmind.sqlite")
    }
}

pub fn sqlite_database_path() -> PathBuf {
    database_path()
}

fn normalize_directory_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

fn is_virtual_directory(path: &str) -> bool {
    normalize_directory_path(path).starts_with("virtual://")
}

fn is_path_within_dir(path: &str, dir: &str) -> bool {
    let normalized_path = normalize_directory_path(path);
    let normalized_dir = normalize_directory_path(dir);
    if normalized_path == normalized_dir {
        return true;
    }

    normalized_path.starts_with(&format!("{normalized_dir}/"))
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
