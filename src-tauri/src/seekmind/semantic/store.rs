#![allow(dead_code)]
/**
 * @author MorningSun
 * @CreatedDate 2026/06/12
 * @Description 语义模型状态、向量重建与语义检索存储逻辑。
 */
use chrono::Utc;
use sqlx::Row;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::seekmind::models::{
    EmbeddingModelView, SemanticDebugHitView, SemanticDebugView, SemanticModelStatusView,
};
use crate::seekmind::search::{normalize_query, rewrite_query_terms, rewrite_search_text};
use crate::seekmind::storage::types::{ChunkRecord, ExtractedDocument};
use crate::seekmind::storage::Database;

use super::client::{PythonSemanticClient, SemanticStatus};
use super::embedding::{cosine_similarity, normalize_embedding_text, text_hash, vector_norm};
use super::vector_store::{resolve_vector_store_backend, VectorStoreBackend};

type SemanticProgressEmitter =
    Arc<dyn Fn(crate::seekmind::models::SemanticRebuildProgressView) + Send + Sync>;

const SEMANTIC_SOURCE_REBUILD: &str = "rebuild";
const CURRENT_VECTOR_INDEX_SCHEMA_VERSION: i64 = 1;
const EMBEDDING_RUNTIME_STATUS_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct CachedEmbeddingRuntimeStatus {
    model_name: String,
    checked_at: Instant,
    status: SemanticStatus,
}

static EMBEDDING_RUNTIME_STATUS_CACHE: OnceLock<Mutex<Option<CachedEmbeddingRuntimeStatus>>> =
    OnceLock::new();

fn cached_embedding_runtime_status(model_name: &str) -> Option<SemanticStatus> {
    let cache = EMBEDDING_RUNTIME_STATUS_CACHE.get_or_init(|| Mutex::new(None));
    let guard = cache.lock().ok()?;
    let cached = guard.as_ref()?;
    if cached.model_name == model_name
        && cached.checked_at.elapsed() <= EMBEDDING_RUNTIME_STATUS_CACHE_TTL
    {
        return Some(cached.status.clone());
    }
    None
}

fn remember_embedding_runtime_status(model_name: &str, status: &SemanticStatus) {
    let cache = EMBEDDING_RUNTIME_STATUS_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some(CachedEmbeddingRuntimeStatus {
            model_name: model_name.to_string(),
            checked_at: Instant::now(),
            status: status.clone(),
        });
    }
}

fn emit_semantic_progress(
    emitter: Option<&SemanticProgressEmitter>,
    payload: crate::seekmind::models::SemanticRebuildProgressView,
) {
    if let Some(callback) = emitter {
        (callback)(payload);
    }
}

fn emit_semantic_embedding_progress(
    progress: Option<&SemanticProgressEmitter>,
    job_id: &str,
    source: &str,
    model: &EmbeddingModelView,
    total_chunks: usize,
    base_processed: usize,
    base_embedded: usize,
    document_path: &str,
    message: String,
    current_chunk: String,
    processed_in_doc: usize,
) {
    emit_semantic_progress(
        progress,
        crate::seekmind::models::SemanticRebuildProgressView {
            job_id: job_id.to_string(),
            state: "running".to_string(),
            message,
            source: source.to_string(),
            model: model.clone(),
            total_chunks,
            processed_chunks: base_processed.saturating_add(processed_in_doc),
            embedded_chunks: base_embedded.saturating_add(processed_in_doc),
            current_document: document_path.to_string(),
            current_chunk,
            percent: progress_percent(
                base_processed.saturating_add(processed_in_doc),
                total_chunks,
            ),
            last_error: String::new(),
            updated_at: format_unix_ts(now_unix_ts()),
        },
    );
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct EmbeddingModelRow {
    id: String,
    name: String,
    provider: String,
    model_path: String,
    dimension: i64,
    enabled: i64,
    available: i64,
    is_default: i64,
    status: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct VectorIndexMetaRow {
    model_id: String,
    chunk_count: i64,
    last_indexed_at: i64,
    last_error: String,
    status: String,
    schema_version: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ChunkEmbeddingRow {
    chunk_id: String,
    document_id: String,
    model_id: String,
    vector_json: String,
    dimension: i64,
    text_hash: String,
    status: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SemanticDebugJoinRow {
    chunk_id: String,
    document_path: String,
    file_name: String,
    heading: String,
    snippet: String,
    paragraph: Option<i64>,
    page: Option<i64>,
    vector_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SemanticSearchRow {
    chunk_id: String,
    vector_json: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SqliteVecSearchRow {
    chunk_id: String,
    document_path: String,
    file_name: String,
    heading: String,
    snippet: String,
    paragraph: Option<i64>,
    page: Option<i64>,
    distance: f32,
}

pub async fn get_embedding_model_status(
    database: &Database,
) -> Result<SemanticModelStatusView, String> {
    let model = load_default_model(database).await?;
    let client = PythonSemanticClient::from_env();
    let runtime_status = client.embedding_status(Some(&model.name));

    match runtime_status {
        Ok(status) => {
            eprintln!(
                "[SeekMind] semantic probe ok model={} available={} message={}",
                model.name, status.available, status.message
            );
            remember_embedding_runtime_status(&model.name, &status);
            sync_model_runtime(database, &model.id, status.available, &status.message).await?;
        }
        Err(error) => {
            // 修复：设置页首屏必须把真实探测失败同步回模型状态，否则前端会继续显示旧的 available 标记，误导用户以为 embedding 可用。
            eprintln!(
                "[SeekMind] semantic probe failed model={} error={}",
                model.name, error
            );
            sync_model_runtime(database, &model.id, false, &error.to_string()).await?;
        }
    }

    let refreshed_model = load_default_model(database).await?;
    let sqlite_chunks = count_sqlite_chunks(database).await?;
    let embedded_chunks = count_embedded_chunks(database, &refreshed_model.id).await?;
    let meta = load_vector_meta(database).await?;

    Ok(SemanticModelStatusView {
        needs_rebuild: sqlite_chunks != embedded_chunks || !refreshed_model.available,
        sqlite_chunks,
        embedded_chunks,
        last_indexed_at: format_unix_ts(meta.last_indexed_at),
        last_error: meta.last_error,
        index_status: meta.status,
        model: refreshed_model,
    })
}

pub async fn list_embedding_models(database: &Database) -> Result<Vec<EmbeddingModelView>, String> {
    let rows = sqlx::query_as::<_, EmbeddingModelRow>(
        r#"
        SELECT id, name, provider, model_path, dimension, enabled, available, is_default, status, created_at, updated_at
        FROM embedding_models
        ORDER BY is_default DESC, updated_at DESC, name ASC
        "#,
    )
    .fetch_all(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    Ok(rows.into_iter().map(model_row_to_view).collect())
}

pub async fn set_default_embedding_model(
    database: &Database,
    model_id: &str,
) -> Result<SemanticModelStatusView, String> {
    let now = now_unix_ts();
    let exists = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM embedding_models
        WHERE id = ?
        "#,
    )
    .bind(model_id)
    .fetch_one(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    if exists == 0 {
        return Err(format!("找不到 embedding 模型: {model_id}"));
    }

    sqlx::query("UPDATE embedding_models SET is_default = 0, updated_at = ?")
        .bind(now)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        UPDATE embedding_models
        SET is_default = 1,
            status = CASE WHEN available = 1 THEN 'ready' ELSE 'unavailable' END,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(model_id)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        UPDATE vector_index_meta
        SET model_id = ?,
            status = 'needs_rebuild',
            last_error = '模型已切换，需要重建语义向量'
        WHERE id = 1
        "#,
    )
    .bind(model_id)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    clear_all_embeddings(database).await?;
    get_embedding_model_status(database).await
}

pub async fn rebuild_all_embeddings(
    database: &Database,
    job_id: String,
    progress: Option<SemanticProgressEmitter>,
) -> Result<SemanticModelStatusView, String> {
    match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => {
            sqlite_vec_rebuild_all_embeddings(database, job_id, progress).await
        }
        VectorStoreBackend::LegacyJson => {
            legacy_rebuild_all_embeddings(database, job_id, progress).await
        }
        backend => {
            // 修复：当前只接入 sqlite-vec，其他后端先回退到已验证的 JSON 实现。
            eprintln!(
                "[SeekMind] vector store backend={} not wired for rebuild yet, using legacy-json",
                backend.label()
            );
            legacy_rebuild_all_embeddings(database, job_id, progress).await
        }
    }
}

async fn legacy_rebuild_all_embeddings(
    database: &Database,
    job_id: String,
    progress: Option<SemanticProgressEmitter>,
) -> Result<SemanticModelStatusView, String> {
    let model = load_default_model(database).await?;
    let client = PythonSemanticClient::from_env();
    if !model.available {
        return Err("embedding 模型不可用，请先确认语义面板显示为可用".to_string());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            d.id AS document_id,
            d.path AS document_path,
            d.file_name AS file_name,
            d.content_hash AS content_hash,
            c.id AS chunk_id,
            c.heading AS heading,
            c.snippet AS snippet,
            c.paragraph AS paragraph,
            c.page AS page
        FROM documents d
        INNER JOIN chunks c ON c.document_id = d.id
        ORDER BY d.path, c.rowid
        "#,
    )
    .fetch_all(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let total_chunks = rows.len();
    let mut processed_chunks = 0usize;
    let mut embedded_chunks = 0usize;
    let mut current_document_id = String::new();
    let mut current_document_path = String::new();
    let mut current_file_name = String::new();
    let mut document_chunks: Vec<(String, String, String, String)> = Vec::new();

    eprintln!(
        "[SeekMind] sqlite-vec rebuild start model={} total_chunks={} job_id={}",
        model.name, total_chunks, job_id
    );
    emit_semantic_progress(
        progress.as_ref(),
        crate::seekmind::models::SemanticRebuildProgressView {
            job_id: job_id.clone(),
            state: "running".to_string(),
            message: "正在准备语义模型".to_string(),
            source: SEMANTIC_SOURCE_REBUILD.to_string(),
            model: model.clone(),
            total_chunks,
            processed_chunks,
            embedded_chunks,
            current_document: String::new(),
            current_chunk: String::new(),
            percent: 0,
            last_error: String::new(),
            updated_at: format_unix_ts(now_unix_ts()),
        },
    );

    client
        .embed_texts(&["SeekMind semantic warmup".to_string()], Some(&model.name))
        .map_err(|error| error.to_string())?;

    clear_all_embeddings(database).await?;

    for row in rows {
        let document_id: String = row
            .try_get("document_id")
            .map_err(|error| error.to_string())?;
        let document_path: String = row
            .try_get("document_path")
            .map_err(|error| error.to_string())?;
        let file_name_value: String = row
            .try_get("file_name")
            .map_err(|error| error.to_string())?;
        let chunk_id: String = row.try_get("chunk_id").map_err(|error| error.to_string())?;
        let heading: String = row.try_get("heading").unwrap_or_default();
        let snippet: String = row.try_get("snippet").unwrap_or_default();

        if current_document_id.is_empty() {
            current_document_id = document_id.clone();
            current_document_path = document_path.clone();
            current_file_name = file_name_value.clone();
        }

        if current_document_id != document_id {
            upsert_document_embeddings_from_rows(
                database,
                &current_document_id,
                &current_document_path,
                &current_file_name,
                &document_chunks,
                &model,
                &client,
                &job_id,
                SEMANTIC_SOURCE_REBUILD,
                progress.as_ref(),
                total_chunks,
                &mut processed_chunks,
                &mut embedded_chunks,
            )
            .await?;
            current_document_id = document_id.clone();
            current_document_path = document_path.clone();
            current_file_name = file_name_value.clone();
            document_chunks.clear();
        }

        document_chunks.push((chunk_id, file_name_value, heading, snippet));
    }

    if !current_document_id.is_empty() {
        upsert_document_embeddings_from_rows(
            database,
            &current_document_id,
            &current_document_path,
            &current_file_name,
            &document_chunks,
            &model,
            &client,
            &job_id,
            SEMANTIC_SOURCE_REBUILD,
            progress.as_ref(),
            total_chunks,
            &mut processed_chunks,
            &mut embedded_chunks,
        )
        .await?;
    }

    update_vector_index_meta(database, &model.id, "ready", "").await?;
    emit_semantic_progress(
        progress.as_ref(),
        crate::seekmind::models::SemanticRebuildProgressView {
            job_id,
            state: "completed".to_string(),
            message: "语义向量重建完成".to_string(),
            source: SEMANTIC_SOURCE_REBUILD.to_string(),
            model: load_default_model(database).await?,
            total_chunks,
            processed_chunks,
            embedded_chunks,
            current_document: current_document_path,
            current_chunk: String::new(),
            percent: 100,
            last_error: String::new(),
            updated_at: format_unix_ts(now_unix_ts()),
        },
    );
    get_embedding_model_status(database).await
}

async fn sqlite_vec_rebuild_all_embeddings(
    database: &Database,
    job_id: String,
    progress: Option<SemanticProgressEmitter>,
) -> Result<SemanticModelStatusView, String> {
    let model = load_default_model(database).await?;
    let client = PythonSemanticClient::from_env();
    if !model.available {
        return Err("embedding 模型不可用，请先确认语义面板显示为可用".to_string());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            d.id AS document_id,
            d.path AS document_path,
            d.file_name AS file_name,
            d.content_hash AS content_hash,
            c.id AS chunk_id,
            c.heading AS heading,
            c.snippet AS snippet,
            c.paragraph AS paragraph,
            c.page AS page
        FROM documents d
        INNER JOIN chunks c ON c.document_id = d.id
        ORDER BY d.path, c.rowid
        "#,
    )
    .fetch_all(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let total_chunks = rows.len();
    let mut processed_chunks = 0usize;
    let mut embedded_chunks = 0usize;
    let mut current_document_id = String::new();
    let mut current_document_path = String::new();
    let mut current_file_name = String::new();
    let mut document_chunks: Vec<(String, String, String, String)> = Vec::new();

    emit_semantic_progress(
        progress.as_ref(),
        crate::seekmind::models::SemanticRebuildProgressView {
            job_id: job_id.clone(),
            state: "running".to_string(),
            message: "正在准备语义模型".to_string(),
            source: SEMANTIC_SOURCE_REBUILD.to_string(),
            model: model.clone(),
            total_chunks,
            processed_chunks,
            embedded_chunks,
            current_document: String::new(),
            current_chunk: String::new(),
            percent: 0,
            last_error: String::new(),
            updated_at: format_unix_ts(now_unix_ts()),
        },
    );

    client
        .embed_texts(&["SeekMind semantic warmup".to_string()], Some(&model.name))
        .map_err(|error| error.to_string())?;

    clear_all_embeddings(database).await?;

    for row in rows {
        let document_id: String = row
            .try_get("document_id")
            .map_err(|error| error.to_string())?;
        let document_path: String = row
            .try_get("document_path")
            .map_err(|error| error.to_string())?;
        let file_name_value: String = row
            .try_get("file_name")
            .map_err(|error| error.to_string())?;
        let chunk_id: String = row.try_get("chunk_id").map_err(|error| error.to_string())?;
        let heading: String = row.try_get("heading").unwrap_or_default();
        let snippet: String = row.try_get("snippet").unwrap_or_default();

        if current_document_id.is_empty() {
            current_document_id = document_id.clone();
            current_document_path = document_path.clone();
            current_file_name = file_name_value.clone();
        }

        if current_document_id != document_id {
            sqlite_vec_upsert_document_embeddings_from_rows(
                database,
                &current_document_id,
                &current_document_path,
                &current_file_name,
                &document_chunks,
                &model,
                &client,
                &job_id,
                SEMANTIC_SOURCE_REBUILD,
                progress.as_ref(),
                total_chunks,
                &mut processed_chunks,
                &mut embedded_chunks,
            )
            .await?;
            current_document_id = document_id.clone();
            current_document_path = document_path.clone();
            current_file_name = file_name_value.clone();
            document_chunks.clear();
        }

        document_chunks.push((chunk_id, file_name_value, heading, snippet));
    }

    if !current_document_id.is_empty() {
        sqlite_vec_upsert_document_embeddings_from_rows(
            database,
            &current_document_id,
            &current_document_path,
            &current_file_name,
            &document_chunks,
            &model,
            &client,
            &job_id,
            SEMANTIC_SOURCE_REBUILD,
            progress.as_ref(),
            total_chunks,
            &mut processed_chunks,
            &mut embedded_chunks,
        )
        .await?;
    }

    update_vector_index_meta(database, &model.id, "ready", "").await?;
    emit_semantic_progress(
        progress.as_ref(),
        crate::seekmind::models::SemanticRebuildProgressView {
            job_id,
            state: "completed".to_string(),
            message: "语义向量重建完成".to_string(),
            source: SEMANTIC_SOURCE_REBUILD.to_string(),
            model: load_default_model(database).await?,
            total_chunks,
            processed_chunks,
            embedded_chunks,
            current_document: current_document_path,
            current_chunk: String::new(),
            percent: 100,
            last_error: String::new(),
            updated_at: format_unix_ts(now_unix_ts()),
        },
    );
    get_embedding_model_status(database).await
}

async fn upsert_document_embeddings_from_rows(
    database: &Database,
    document_id: &str,
    document_path: &str,
    file_name: &str,
    chunks: &[(String, String, String, String)],
    model: &EmbeddingModelView,
    client: &PythonSemanticClient,
    job_id: &str,
    source: &str,
    progress: Option<&SemanticProgressEmitter>,
    total_chunks: usize,
    processed_chunks: &mut usize,
    embedded_chunks: &mut usize,
) -> Result<(), String> {
    sqlx::query("DELETE FROM chunk_embeddings WHERE document_id = ?")
        .bind(document_id)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;

    let semantic_texts: Vec<String> = chunks
        .iter()
        .map(|(_, file_name_value, heading, snippet)| {
            normalize_embedding_text(&format!("{file_name_value}\n{heading}\n{snippet}"))
        })
        .collect();

    emit_semantic_progress(
        progress,
        crate::seekmind::models::SemanticRebuildProgressView {
            job_id: job_id.to_string(),
            state: "running".to_string(),
            message: format!("正在处理 {file_name}"),
            source: source.to_string(),
            model: model.clone(),
            total_chunks,
            processed_chunks: *processed_chunks,
            embedded_chunks: *embedded_chunks,
            current_document: document_path.to_string(),
            current_chunk: String::new(),
            percent: progress_percent(*processed_chunks, total_chunks),
            last_error: String::new(),
            updated_at: format_unix_ts(now_unix_ts()),
        },
    );

    let base_processed = *processed_chunks;
    let base_embedded = *embedded_chunks;
    emit_semantic_embedding_progress(
        progress,
        job_id,
        SEMANTIC_SOURCE_REBUILD,
        model,
        total_chunks,
        base_processed,
        base_embedded,
        document_path,
        format!("正在生成 {file_name} 的语义向量"),
        "准备生成".to_string(),
        0,
    );

    let vectors = client
        .embed_texts_stream(&semantic_texts, Some(&model.name), |event| {
            if event.kind != "event" {
                return;
            }

            let processed_in_doc = event.processed.min(semantic_texts.len());
            let current_chunk = if event.current.is_empty() {
                format!("{processed_in_doc}/{}", semantic_texts.len())
            } else {
                event.current.clone()
            };
            let message = if event.message.is_empty() {
                format!("正在生成 {file_name} 的语义向量")
            } else {
                event.message.clone()
            };
            emit_semantic_embedding_progress(
                progress,
                job_id,
                source,
                model,
                total_chunks,
                base_processed,
                base_embedded,
                document_path,
                message,
                current_chunk,
                processed_in_doc,
            );
        })
        .map_err(|error| error.to_string())?;

    if vectors.len() != chunks.len() {
        return Err(format!(
            "embedding 向量数量不匹配: chunks={} vectors={}",
            chunks.len(),
            vectors.len()
        ));
    }

    emit_semantic_embedding_progress(
        progress,
        job_id,
        SEMANTIC_SOURCE_REBUILD,
        model,
        total_chunks,
        base_processed,
        base_embedded,
        document_path,
        format!("正在写入 {file_name} 的语义索引"),
        String::new(),
        semantic_texts.len(),
    );

    for (index, _chunk) in chunks.iter().enumerate() {
        let chunk_id = format!("{document_id}:{index}");
        let semantic_text = &semantic_texts[index];
        let vector_json =
            serde_json::to_string(&vectors[index]).map_err(|error| error.to_string())?;
        let now = now_unix_ts();

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO chunk_embeddings
                (chunk_id, document_id, model_id, vector_json, dimension, text_hash, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, 'ready', ?, ?)
            "#,
        )
        .bind(&chunk_id)
        .bind(document_id)
        .bind(&model.id)
        .bind(vector_json)
        .bind(model.dimension as i64)
        .bind(text_hash(semantic_text))
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;
    }

    *processed_chunks += semantic_texts.len();
    *embedded_chunks += semantic_texts.len();

    update_vector_index_meta(database, &model.id, "ready", "").await?;
    Ok(())
}

async fn sqlite_vec_upsert_document_embeddings_from_rows(
    database: &Database,
    document_id: &str,
    document_path: &str,
    file_name: &str,
    chunks: &[(String, String, String, String)],
    model: &EmbeddingModelView,
    client: &PythonSemanticClient,
    job_id: &str,
    source: &str,
    progress: Option<&SemanticProgressEmitter>,
    total_chunks: usize,
    processed_chunks: &mut usize,
    embedded_chunks: &mut usize,
) -> Result<(), String> {
    eprintln!(
        "[SeekMind] sqlite-vec embedding document path={} chunks={}",
        document_path,
        chunks.len()
    );
    let semantic_texts: Vec<String> = chunks
        .iter()
        .map(|(_, file_name_value, heading, snippet)| {
            normalize_embedding_text(&format!("{file_name_value}\n{heading}\n{snippet}"))
        })
        .collect();

    emit_semantic_progress(
        progress,
        crate::seekmind::models::SemanticRebuildProgressView {
            job_id: job_id.to_string(),
            state: "running".to_string(),
            message: format!("正在处理 {file_name}"),
            source: source.to_string(),
            model: model.clone(),
            total_chunks,
            processed_chunks: *processed_chunks,
            embedded_chunks: *embedded_chunks,
            current_document: document_path.to_string(),
            current_chunk: String::new(),
            percent: progress_percent(*processed_chunks, total_chunks),
            last_error: String::new(),
            updated_at: format_unix_ts(now_unix_ts()),
        },
    );

    let base_processed = *processed_chunks;
    let base_embedded = *embedded_chunks;
    emit_semantic_embedding_progress(
        progress,
        job_id,
        SEMANTIC_SOURCE_REBUILD,
        model,
        total_chunks,
        base_processed,
        base_embedded,
        document_path,
        format!("正在生成 {file_name} 的语义向量"),
        "准备生成".to_string(),
        0,
    );

    let vectors = client
        .embed_texts_stream(&semantic_texts, Some(&model.name), |event| {
            if event.kind != "event" {
                return;
            }

            let processed_in_doc = event.processed.min(semantic_texts.len());
            let current_chunk = if event.current.is_empty() {
                format!("{processed_in_doc}/{}", semantic_texts.len())
            } else {
                event.current.clone()
            };
            let message = if event.message.is_empty() {
                format!("正在生成 {file_name} 的语义向量")
            } else {
                event.message.clone()
            };
            emit_semantic_embedding_progress(
                progress,
                job_id,
                source,
                model,
                total_chunks,
                base_processed,
                base_embedded,
                document_path,
                message,
                current_chunk,
                processed_in_doc,
            );
        })
        .map_err(|error| error.to_string())?;

    if vectors.len() != chunks.len() {
        return Err(format!(
            "embedding 向量数量不匹配: chunks={} vectors={}",
            chunks.len(),
            vectors.len()
        ));
    }

    emit_semantic_embedding_progress(
        progress,
        job_id,
        SEMANTIC_SOURCE_REBUILD,
        model,
        total_chunks,
        base_processed,
        base_embedded,
        document_path,
        format!("正在写入 {file_name} 的语义索引"),
        String::new(),
        semantic_texts.len(),
    );

    let mut tx = database
        .pool()
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        DELETE FROM chunk_embedding_vec
        WHERE rowid IN (
            SELECT rowid
            FROM chunk_embedding_meta
            WHERE document_id = ?
        )
        "#,
    )
    .bind(document_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM chunk_embedding_meta WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

    for (index, _chunk) in chunks.iter().enumerate() {
        let chunk_id = format!("{document_id}:{index}");
        let semantic_text = &semantic_texts[index];
        let vector_json =
            serde_json::to_string(&vectors[index]).map_err(|error| error.to_string())?;
        let now = now_unix_ts();

        sqlx::query(
            r#"
            INSERT INTO chunk_embedding_meta
                (chunk_id, document_id, model_id, text_hash, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, 'ready', ?, ?)
            "#,
        )
        .bind(&chunk_id)
        .bind(document_id)
        .bind(&model.id)
        .bind(text_hash(semantic_text))
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

        let rowid: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;

        sqlx::query(
            r#"
            INSERT INTO chunk_embedding_vec
                (rowid, embedding)
            VALUES (?, vec_f32(?))
            "#,
        )
        .bind(rowid)
        .bind(vector_json)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    tx.commit().await.map_err(|error| error.to_string())?;

    *processed_chunks += semantic_texts.len();
    *embedded_chunks += semantic_texts.len();
    update_vector_index_meta(database, &model.id, "ready", "").await?;
    Ok(())
}

fn progress_percent(processed: usize, total: usize) -> u8 {
    if total == 0 {
        return 0;
    }

    let ratio = (processed as f32 / total as f32) * 100.0;
    ratio.round().clamp(0.0, 100.0) as u8
}

pub async fn upsert_document_embeddings(
    database: &Database,
    document_id: &str,
    document: &ExtractedDocument,
    chunks: &[ChunkRecord],
    job_id: &str,
    source: &str,
    progress: Option<&SemanticProgressEmitter>,
) -> Result<(), String> {
    match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => {
            sqlite_vec_upsert_document_embeddings(
                database,
                document_id,
                document,
                chunks,
                job_id,
                source,
                progress,
            )
            .await
        }
        VectorStoreBackend::LegacyJson => {
            legacy_upsert_document_embeddings(
                database,
                document_id,
                document,
                chunks,
                job_id,
                source,
                progress,
            )
            .await
        }
        backend => {
            // 修复：当前只接入 sqlite-vec，其他后端保持旧实现可用。
            eprintln!(
                "[SeekMind] vector store backend={} not wired for upsert yet, using legacy-json",
                backend.label()
            );
            legacy_upsert_document_embeddings(
                database,
                document_id,
                document,
                chunks,
                job_id,
                source,
                progress,
            )
            .await
        }
    }
}

async fn legacy_upsert_document_embeddings(
    database: &Database,
    document_id: &str,
    document: &ExtractedDocument,
    chunks: &[ChunkRecord],
    job_id: &str,
    source: &str,
    progress: Option<&SemanticProgressEmitter>,
) -> Result<(), String> {
    let model = load_default_model(database).await?;
    let client = PythonSemanticClient::from_env();
    let runtime_status = match cached_embedding_runtime_status(&model.name) {
        Some(status) => status,
        None => {
            // 修复：索引批量写入时避免每个文档都重新启动 Python sidecar 做 embedding runtime 探测。
            let status = client
                .embedding_status(Some(&model.name))
                .map_err(|error| error.to_string())?;
            remember_embedding_runtime_status(&model.name, &status);
            sync_model_runtime(database, &model.id, status.available, &status.message).await?;
            status
        }
    };
    let model = load_default_model(database).await?;
    if !model.enabled || !model.available {
        return Err(format!("embedding 模型不可用: {}", runtime_status.message));
    }

    sqlx::query("DELETE FROM chunk_embeddings WHERE document_id = ?")
        .bind(document_id)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;

    let semantic_texts: Vec<String> = chunks
        .iter()
        .map(|chunk| {
            normalize_embedding_text(&format!(
                "{}\n{}\n{}",
                document.file_name, chunk.heading, chunk.snippet
            ))
        })
        .collect();

    emit_semantic_embedding_progress(
        progress,
        job_id,
        source,
        &model,
        semantic_texts.len(),
        0,
        0,
        &document.path,
        format!("正在生成 {} 的语义向量", document.file_name),
        "准备生成".to_string(),
        0,
    );

    let vectors = client
        .embed_texts_stream(&semantic_texts, Some(&model.name), |event| {
            if event.kind != "event" {
                return;
            }

            let processed_in_doc = event.processed.min(semantic_texts.len());
            let current_chunk = if event.current.is_empty() {
                format!("{processed_in_doc}/{}", semantic_texts.len())
            } else {
                event.current.clone()
            };
            let message = if event.message.is_empty() {
                format!("正在生成 {} 的语义向量", document.file_name)
            } else {
                event.message.clone()
            };
            emit_semantic_embedding_progress(
                progress,
                job_id,
                source,
                &model,
                semantic_texts.len(),
                0,
                0,
                &document.path,
                message,
                current_chunk,
                processed_in_doc,
            );
        })
        .map_err(|error| error.to_string())?;

    if vectors.len() != chunks.len() {
        return Err(format!(
            "embedding 向量数量不匹配: chunks={} vectors={}",
            chunks.len(),
            vectors.len()
        ));
    }

    emit_semantic_embedding_progress(
        progress,
        job_id,
        source,
        &model,
        semantic_texts.len(),
        0,
        0,
        &document.path,
        format!("正在写入 {} 的语义索引", document.file_name),
        String::new(),
        semantic_texts.len(),
    );

    for (index, _chunk) in chunks.iter().enumerate() {
        let chunk_id = format!("{document_id}:{index}");
        let semantic_text = &semantic_texts[index];
        let vector_json =
            serde_json::to_string(&vectors[index]).map_err(|error| error.to_string())?;
        let now = now_unix_ts();

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO chunk_embeddings
                (chunk_id, document_id, model_id, vector_json, dimension, text_hash, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, 'ready', ?, ?)
            "#,
        )
        .bind(&chunk_id)
        .bind(document_id)
        .bind(&model.id)
        .bind(vector_json)
        .bind(model.dimension as i64)
        .bind(text_hash(semantic_text))
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;
    }

    update_vector_index_meta(database, &model.id, "ready", "").await?;
    Ok(())
}

async fn sqlite_vec_upsert_document_embeddings(
    database: &Database,
    document_id: &str,
    document: &ExtractedDocument,
    chunks: &[ChunkRecord],
    job_id: &str,
    source: &str,
    progress: Option<&SemanticProgressEmitter>,
) -> Result<(), String> {
    let model = load_default_model(database).await?;
    let client = PythonSemanticClient::from_env();
    let runtime_status = match cached_embedding_runtime_status(&model.name) {
        Some(status) => status,
        None => {
            // 修复：索引批量写入时避免每个文档都重新启动 Python sidecar 做 embedding runtime 探测。
            let status = client
                .embedding_status(Some(&model.name))
                .map_err(|error| error.to_string())?;
            remember_embedding_runtime_status(&model.name, &status);
            sync_model_runtime(database, &model.id, status.available, &status.message).await?;
            status
        }
    };
    let model = load_default_model(database).await?;
    if !model.enabled || !model.available {
        return Err(format!("embedding 模型不可用: {}", runtime_status.message));
    }

    let semantic_texts: Vec<String> = chunks
        .iter()
        .map(|chunk| {
            normalize_embedding_text(&format!(
                "{}\n{}\n{}",
                document.file_name, chunk.heading, chunk.snippet
            ))
        })
        .collect();

    emit_semantic_embedding_progress(
        progress,
        job_id,
        source,
        &model,
        semantic_texts.len(),
        0,
        0,
        &document.path,
        format!("正在生成 {} 的语义向量", document.file_name),
        "准备生成".to_string(),
        0,
    );

    let vectors = client
        .embed_texts_stream(&semantic_texts, Some(&model.name), |event| {
            if event.kind != "event" {
                return;
            }

            let processed_in_doc = event.processed.min(semantic_texts.len());
            let current_chunk = if event.current.is_empty() {
                format!("{processed_in_doc}/{}", semantic_texts.len())
            } else {
                event.current.clone()
            };
            let message = if event.message.is_empty() {
                format!("正在生成 {} 的语义向量", document.file_name)
            } else {
                event.message.clone()
            };
            emit_semantic_embedding_progress(
                progress,
                job_id,
                source,
                &model,
                semantic_texts.len(),
                0,
                0,
                &document.path,
                message,
                current_chunk,
                processed_in_doc,
            );
        })
        .map_err(|error| error.to_string())?;

    if vectors.len() != chunks.len() {
        return Err(format!(
            "embedding 向量数量不匹配: chunks={} vectors={}",
            chunks.len(),
            vectors.len()
        ));
    }

    emit_semantic_embedding_progress(
        progress,
        job_id,
        source,
        &model,
        semantic_texts.len(),
        0,
        0,
        &document.path,
        format!("正在写入 {} 的语义索引", document.file_name),
        String::new(),
        semantic_texts.len(),
    );

    let mut tx = database
        .pool()
        .begin()
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"
        DELETE FROM chunk_embedding_vec
        WHERE rowid IN (
            SELECT rowid
            FROM chunk_embedding_meta
            WHERE document_id = ?
        )
        "#,
    )
    .bind(document_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM chunk_embedding_meta WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

    for (index, _chunk) in chunks.iter().enumerate() {
        let chunk_id = format!("{document_id}:{index}");
        let semantic_text = &semantic_texts[index];
        let vector_json =
            serde_json::to_string(&vectors[index]).map_err(|error| error.to_string())?;
        let now = now_unix_ts();

        sqlx::query(
            r#"
            INSERT INTO chunk_embedding_meta
                (chunk_id, document_id, model_id, text_hash, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, 'ready', ?, ?)
            "#,
        )
        .bind(&chunk_id)
        .bind(document_id)
        .bind(&model.id)
        .bind(text_hash(semantic_text))
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

        let rowid: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;

        sqlx::query(
            r#"
            INSERT INTO chunk_embedding_vec
                (rowid, embedding)
            VALUES (?, ?)
            "#,
        )
        .bind(rowid)
        .bind(vector_json)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    tx.commit().await.map_err(|error| error.to_string())?;
    update_vector_index_meta(database, &model.id, "ready", "").await?;
    Ok(())
}

pub async fn delete_document_embeddings(database: &Database, path: &str) -> Result<(), String> {
    match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => {
            sqlite_vec_delete_document_embeddings(database, path).await
        }
        VectorStoreBackend::LegacyJson => legacy_delete_document_embeddings(database, path).await,
        backend => {
            // 修复：删除文档时仍需保证旧实现可回退，避免后端切换期间出现残留向量。
            eprintln!(
                "[SeekMind] vector store backend={} not wired for delete yet, using legacy-json",
                backend.label()
            );
            legacy_delete_document_embeddings(database, path).await
        }
    }
}

async fn legacy_delete_document_embeddings(database: &Database, path: &str) -> Result<(), String> {
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
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn sqlite_vec_delete_document_embeddings(
    database: &Database,
    path: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"
        DELETE FROM chunk_embedding_vec
        WHERE rowid IN (
            SELECT meta.rowid
            FROM chunk_embedding_meta meta
            INNER JOIN documents d ON d.id = meta.document_id
            WHERE d.path = ?
        )
        "#,
    )
    .bind(path)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        DELETE FROM chunk_embedding_meta
        WHERE document_id IN (
            SELECT id
            FROM documents
            WHERE path = ?
        )
        "#,
    )
    .bind(path)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn delete_directory_embeddings(
    database: &Database,
    dir_path: &str,
) -> Result<(), String> {
    match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => {
            sqlite_vec_delete_directory_embeddings(database, dir_path).await
        }
        VectorStoreBackend::LegacyJson => {
            legacy_delete_directory_embeddings(database, dir_path).await
        }
        backend => {
            // 修复：目录删除也必须沿用可回退路径，避免向量残留影响后续搜索。
            eprintln!(
                "[SeekMind] vector store backend={} not wired for directory delete yet, using legacy-json",
                backend.label()
            );
            legacy_delete_directory_embeddings(database, dir_path).await
        }
    }
}

async fn legacy_delete_directory_embeddings(
    database: &Database,
    dir_path: &str,
) -> Result<(), String> {
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
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn sqlite_vec_delete_directory_embeddings(
    database: &Database,
    dir_path: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"
        DELETE FROM chunk_embedding_vec
        WHERE rowid IN (
            SELECT meta.rowid
            FROM chunk_embedding_meta meta
            INNER JOIN documents d ON d.id = meta.document_id
            WHERE d.dir_path = ?
        )
        "#,
    )
    .bind(dir_path)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        DELETE FROM chunk_embedding_meta
        WHERE document_id IN (
            SELECT id
            FROM documents
            WHERE dir_path = ?
        )
        "#,
    )
    .bind(dir_path)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn clear_all_embeddings(database: &Database) -> Result<(), String> {
    match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => sqlite_vec_clear_all_embeddings(database).await,
        VectorStoreBackend::LegacyJson => legacy_clear_all_embeddings(database).await,
        backend => {
            // 修复：清空索引必须先保持稳定回退，直到正式向量库完成接入和验证。
            eprintln!(
                "[SeekMind] vector store backend={} not wired for clear yet, using legacy-json",
                backend.label()
            );
            legacy_clear_all_embeddings(database).await
        }
    }
}

async fn legacy_clear_all_embeddings(database: &Database) -> Result<(), String> {
    sqlx::query("DELETE FROM chunk_embeddings")
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;

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
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn sqlite_vec_clear_all_embeddings(database: &Database) -> Result<(), String> {
    sqlx::query("DELETE FROM chunk_embedding_vec")
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM chunk_embedding_meta")
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;

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
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn semantic_debug_report(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<SemanticDebugView, String> {
    match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => {
            sqlite_vec_semantic_debug_report(database, query, limit).await
        }
        VectorStoreBackend::LegacyJson => {
            legacy_semantic_debug_report(database, query, limit).await
        }
        backend => {
            // 修复：调试报表应与实际运行路径一致，后端未接入时明确回退到 legacy-json。
            eprintln!(
                "[SeekMind] vector store backend={} not wired for debug report yet, using legacy-json",
                backend.label()
            );
            legacy_semantic_debug_report(database, query, limit).await
        }
    }
}

async fn legacy_semantic_debug_report(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<SemanticDebugView, String> {
    let model = load_default_model(database).await?;
    let index_settings = database
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    let normalized_query = normalize_query(query).join(" ");
    let rewritten_terms = rewrite_query_terms(query);
    let rewritten_query = rewrite_search_text(query);
    let query_rewrite_applied = !query.trim().is_empty() && !rewritten_query.trim().is_empty();
    let client = PythonSemanticClient::from_env();
    let query_status = match client.embedding_status(Some(&model.name)) {
        Ok(status) => status,
        Err(error) => {
            let message = error.to_string();
            sync_model_runtime(database, &model.id, false, &message).await?;
            PythonSemanticClient::from_env()
                .embedding_status(Some(&model.name))
                .unwrap_or(super::client::SemanticStatus {
                    available: false,
                    provider: "fastembed".to_string(),
                    model_name: model.name.clone(),
                    model_path: model.model_path.clone(),
                    dimension: model.dimension,
                    message,
                })
        }
    };
    sync_model_runtime(
        database,
        &model.id,
        query_status.available,
        &query_status.message,
    )
    .await?;
    let model = load_default_model(database).await?;

    let query_vectors = if rewritten_query.trim().is_empty() || !model.available {
        Vec::new()
    } else {
        client
            .embed_texts(&[rewritten_query.clone()], Some(&model.name))
            .map_err(|error| error.to_string())?
    };
    let query_vector = query_vectors.first().cloned().unwrap_or_default();
    let query_vector_dim = query_vector.len();
    let query_vector_norm = vector_norm(&query_vector);
    let query_vector_ready = !query_vector.is_empty() && query_vector_norm > 0.0;

    let sqlite_chunks = count_sqlite_chunks(database).await?;
    let embedded_chunks = count_embedded_chunks(database, &model.id).await?;
    let meta = load_vector_meta(database).await?;

    let mut hits = Vec::new();
    if query_vector_ready {
        let rows = sqlx::query_as::<_, SemanticDebugJoinRow>(
            r#"
            SELECT
                ce.chunk_id AS chunk_id,
                d.path AS document_path,
                d.file_name AS file_name,
                c.heading AS heading,
                c.snippet AS snippet,
                c.paragraph AS paragraph,
                c.page AS page,
                ce.vector_json AS vector_json
            FROM chunk_embeddings ce
            INNER JOIN documents d ON d.id = ce.document_id
            INNER JOIN chunks c ON c.id = ce.chunk_id
            WHERE ce.model_id = ?
            ORDER BY d.path, c.rowid
            "#,
        )
        .bind(&model.id)
        .fetch_all(database.pool())
        .await
        .map_err(|error| error.to_string())?;

        for row in rows {
            let vector = parse_vector_json(&row.vector_json)?;
            let score = cosine_similarity(&query_vector, &vector);
            hits.push(SemanticDebugHitView {
                chunk_id: row.chunk_id,
                document_path: row.document_path,
                file_name: row.file_name,
                heading: row.heading.clone(),
                title_path: row.heading,
                snippet: row.snippet,
                paragraph: row.paragraph.map(|value| value as u32),
                page: row.page.map(|value| value as u32),
                score,
            });
        }

        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(limit.max(1));
    }

    Ok(SemanticDebugView {
        query: query.to_string(),
        normalized_query,
        rewritten_query,
        rewritten_terms,
        query_rewrite_applied,
        query_vector_dim,
        query_vector_ready,
        query_vector_norm,
        model,
        sqlite_chunks,
        embedded_chunks,
        hit_count: hits.len(),
        semantic_threshold: index_settings.semantic_threshold,
        semantic_candidate_count: hits.len(),
        semantic_filtered_count: 0,
        hits,
        index_status: meta.status,
        last_error: meta.last_error,
    })
}

async fn sqlite_vec_semantic_debug_report(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<SemanticDebugView, String> {
    let model = load_default_model(database).await?;
    let index_settings = database
        .get_index_settings()
        .await
        .map_err(|error| error.to_string())?;
    let normalized_query = normalize_query(query).join(" ");
    let rewritten_terms = rewrite_query_terms(query);
    let rewritten_query = rewrite_search_text(query);
    let query_rewrite_applied = !query.trim().is_empty() && !rewritten_query.trim().is_empty();
    let client = PythonSemanticClient::from_env();
    let query_status = match client.embedding_status(Some(&model.name)) {
        Ok(status) => status,
        Err(error) => {
            let message = error.to_string();
            sync_model_runtime(database, &model.id, false, &message).await?;
            PythonSemanticClient::from_env()
                .embedding_status(Some(&model.name))
                .unwrap_or(super::client::SemanticStatus {
                    available: false,
                    provider: "fastembed".to_string(),
                    model_name: model.name.clone(),
                    model_path: model.model_path.clone(),
                    dimension: model.dimension,
                    message,
                })
        }
    };
    sync_model_runtime(
        database,
        &model.id,
        query_status.available,
        &query_status.message,
    )
    .await?;
    let model = load_default_model(database).await?;

    let query_vectors = if rewritten_query.trim().is_empty() || !model.available {
        Vec::new()
    } else {
        client
            .embed_texts(&[rewritten_query.clone()], Some(&model.name))
            .map_err(|error| error.to_string())?
    };
    let query_vector = query_vectors.first().cloned().unwrap_or_default();
    let query_vector_dim = query_vector.len();
    let query_vector_norm = vector_norm(&query_vector);
    let query_vector_ready = !query_vector.is_empty() && query_vector_norm > 0.0;

    let sqlite_chunks = count_sqlite_chunks(database).await?;
    let embedded_chunks = count_embedded_chunks(database, &model.id).await?;
    let meta = load_vector_meta(database).await?;

    let hits = if query_vector_ready {
        sqlite_vec_semantic_search_hits(database, query, limit).await?
    } else {
        Vec::new()
    };

    Ok(SemanticDebugView {
        query: query.to_string(),
        normalized_query,
        rewritten_query,
        rewritten_terms,
        query_rewrite_applied,
        query_vector_dim,
        query_vector_ready,
        query_vector_norm,
        model,
        sqlite_chunks,
        embedded_chunks,
        hit_count: hits.len(),
        semantic_threshold: index_settings.semantic_threshold,
        semantic_candidate_count: hits.len(),
        semantic_filtered_count: 0,
        hits,
        index_status: meta.status,
        last_error: meta.last_error,
    })
}

pub async fn semantic_search_hits(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<Vec<SemanticDebugHitView>, String> {
    match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => {
            sqlite_vec_semantic_search_hits(database, query, limit).await
        }
        VectorStoreBackend::LegacyJson => legacy_semantic_search_hits(database, query, limit).await,
        backend => {
            // 修复：混合检索仍需依赖稳定的回退实现，避免正式向量库接入前搜索结果为空。
            eprintln!(
                "[SeekMind] vector store backend={} not wired for search yet, using legacy-json",
                backend.label()
            );
            legacy_semantic_search_hits(database, query, limit).await
        }
    }
}

async fn legacy_semantic_search_hits(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<Vec<SemanticDebugHitView>, String> {
    let model = load_default_model(database).await?;
    if !model.enabled || !model.available || query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let embedded_chunks: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM chunk_embeddings
        WHERE model_id = ?
        "#,
    )
    .bind(&model.id)
    .fetch_one(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    if embedded_chunks == 0 {
        return Ok(Vec::new());
    }

    let rewritten_query = rewrite_search_text(query);
    if rewritten_query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let client = PythonSemanticClient::from_env();
    let query_vectors = client
        .embed_texts(&[rewritten_query], Some(&model.name))
        .map_err(|error| error.to_string())?;
    let Some(query_vector) = query_vectors.first() else {
        return Ok(Vec::new());
    };
    if query_vector.is_empty() || vector_norm(query_vector) <= 0.0 {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as::<_, SemanticSearchRow>(
        r#"
        SELECT chunk_id, vector_json
        FROM chunk_embeddings
        WHERE model_id = ?
        "#,
    )
    .bind(&model.id)
    .fetch_all(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let mut hits = Vec::new();
    for row in rows {
        let vector = parse_vector_json(&row.vector_json)?;
        let score = cosine_similarity(query_vector, &vector);
        hits.push(SemanticDebugHitView {
            chunk_id: row.chunk_id,
            document_path: String::new(),
            file_name: String::new(),
            heading: String::new(),
            title_path: String::new(),
            snippet: String::new(),
            paragraph: None,
            page: None,
            score,
        });
    }

    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit.max(1));
    Ok(hits)
}

async fn sqlite_vec_semantic_search_hits(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<Vec<SemanticDebugHitView>, String> {
    let model = load_default_model(database).await?;
    if !model.enabled || !model.available || query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let rewritten_query = rewrite_search_text(query);
    if rewritten_query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let client = PythonSemanticClient::from_env();
    let query_vectors = client
        .embed_texts(&[rewritten_query], Some(&model.name))
        .map_err(|error| error.to_string())?;
    let Some(query_vector) = query_vectors.first() else {
        return Ok(Vec::new());
    };
    if query_vector.is_empty() || vector_norm(query_vector) <= 0.0 {
        return Ok(Vec::new());
    }

    eprintln!(
        "[SeekMind] sqlite-vec search query=\"{}\" limit={} model={}",
        query, limit, model.name
    );
    let query_vector_json =
        serde_json::to_string(query_vector).map_err(|error| error.to_string())?;
    // 修复：当前先用 sqlite-vec 的 cosine scalar 做排序，避免 vec0 KNN 在联表查询下出现召回为空。
    let rows = sqlx::query_as::<_, SqliteVecSearchRow>(
        r#"
        SELECT
            meta.chunk_id AS chunk_id,
            d.path AS document_path,
            d.file_name AS file_name,
            c.heading AS heading,
            c.snippet AS snippet,
            c.paragraph AS paragraph,
            c.page AS page,
            vec_distance_cosine(v.embedding, vec_f32(?)) AS distance
        FROM chunk_embedding_vec v
        INNER JOIN chunk_embedding_meta meta ON meta.rowid = v.rowid
        INNER JOIN documents d ON d.id = meta.document_id
        INNER JOIN chunks c ON c.id = meta.chunk_id
        WHERE meta.model_id = ?
        ORDER BY distance ASC
        LIMIT ?
        "#,
    )
    .bind(query_vector_json)
    .bind(&model.id)
    .bind(limit.max(1) as i64)
    .fetch_all(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let mut hits = Vec::with_capacity(rows.len());
    for row in rows {
        hits.push(SemanticDebugHitView {
            chunk_id: row.chunk_id,
            document_path: row.document_path,
            file_name: row.file_name,
            heading: row.heading.clone(),
            title_path: row.heading,
            snippet: row.snippet,
            paragraph: row.paragraph.map(|value| value as u32),
            page: row.page.map(|value| value as u32),
            score: if row.distance <= 0.0 {
                1.0
            } else {
                1.0 / (1.0 + row.distance)
            },
        });
    }
    Ok(hits)
}

async fn load_default_model(database: &Database) -> Result<EmbeddingModelView, String> {
    let row = sqlx::query_as::<_, EmbeddingModelRow>(
        r#"
        SELECT id, name, provider, model_path, dimension, enabled, available, is_default, status, created_at, updated_at
        FROM embedding_models
        WHERE is_default = 1
        ORDER BY updated_at DESC, name ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    let row = if let Some(row) = row {
        row
    } else {
        sqlx::query_as::<_, EmbeddingModelRow>(
            r#"
            SELECT id, name, provider, model_path, dimension, enabled, available, is_default, status, created_at, updated_at
            FROM embedding_models
            ORDER BY is_default DESC, updated_at DESC, name ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(database.pool())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "没有可用的 embedding 模型".to_string())?
    };

    Ok(model_row_to_view(row))
}

async fn load_vector_meta(database: &Database) -> Result<VectorIndexMetaRow, String> {
    let row = sqlx::query_as::<_, VectorIndexMetaRow>(
        r#"
        SELECT model_id, chunk_count, last_indexed_at, last_error, status, schema_version
        FROM vector_index_meta
        WHERE id = 1
        "#,
    )
    .fetch_optional(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    Ok(row.unwrap_or(VectorIndexMetaRow {
        model_id: "default-local-embedding".to_string(),
        chunk_count: 0,
        last_indexed_at: 0,
        last_error: String::new(),
        status: "idle".to_string(),
        schema_version: CURRENT_VECTOR_INDEX_SCHEMA_VERSION,
    }))
}

async fn sync_model_runtime(
    database: &Database,
    model_id: &str,
    available: bool,
    message: &str,
) -> Result<(), String> {
    let now = now_unix_ts();
    sqlx::query(
        r#"
        UPDATE embedding_models
        SET available = ?,
            status = ?,
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(available as i64)
    .bind(if available { "ready" } else { "unavailable" })
    .bind(now)
    .bind(model_id)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        UPDATE vector_index_meta
        SET model_id = ?,
            last_error = ?,
            status = CASE WHEN ? = 1 THEN 'ready' ELSE 'unavailable' END
        WHERE id = 1
        "#,
    )
    .bind(model_id)
    .bind(message)
    .bind(available as i64)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;

    Ok(())
}

async fn update_vector_index_meta(
    database: &Database,
    model_id: &str,
    status: &str,
    last_error: &str,
) -> Result<(), String> {
    let chunk_count = count_embedded_chunks(database, model_id).await?;
    sqlx::query(
        r#"
        UPDATE vector_index_meta
        SET model_id = ?,
            chunk_count = ?,
            last_indexed_at = ?,
            last_error = ?,
            status = ?
        WHERE id = 1
        "#,
    )
    .bind(model_id)
    .bind(chunk_count as i64)
    .bind(now_unix_ts())
    .bind(last_error)
    .bind(status)
    .execute(database.pool())
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn count_sqlite_chunks(database: &Database) -> Result<usize, String> {
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM chunks")
        .fetch_one(database.pool())
        .await
        .map_err(|error| error.to_string())?;
    Ok(count.max(0) as usize)
}

async fn count_embedded_chunks(database: &Database, model_id: &str) -> Result<usize, String> {
    let count = match resolve_vector_store_backend() {
        VectorStoreBackend::SqliteVec => sqlx::query_scalar::<_, i64>(
            r#"
                SELECT COUNT(*)
                FROM chunk_embedding_meta
                WHERE model_id = ?
                "#,
        )
        .bind(model_id)
        .fetch_one(database.pool())
        .await
        .map_err(|error| error.to_string())?,
        _ => sqlx::query_scalar::<_, i64>(
            r#"
                SELECT COUNT(*)
                FROM chunk_embeddings
                WHERE model_id = ?
                "#,
        )
        .bind(model_id)
        .fetch_one(database.pool())
        .await
        .map_err(|error| error.to_string())?,
    };
    Ok(count.max(0) as usize)
}

fn parse_vector_json(raw: &str) -> Result<Vec<f32>, String> {
    serde_json::from_str::<Vec<f32>>(raw).map_err(|error| error.to_string())
}

fn model_row_to_view(row: EmbeddingModelRow) -> EmbeddingModelView {
    EmbeddingModelView {
        id: row.id,
        name: row.name,
        provider: row.provider,
        model_path: row.model_path,
        dimension: row.dimension.max(0) as usize,
        enabled: row.enabled != 0,
        available: row.available != 0,
        is_default: row.is_default != 0,
        status: row.status,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn now_unix_ts() -> i64 {
    Utc::now().timestamp()
}

fn format_unix_ts(timestamp: i64) -> String {
    if timestamp <= 0 {
        return String::new();
    }

    chrono::DateTime::<Utc>::from_timestamp(timestamp, 0)
        .map(|datetime| datetime.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}
