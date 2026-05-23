#![allow(dead_code)]

use chrono::Utc;
use sqlx::Row;

use crate::docmind::models::{
    EmbeddingModelView, SemanticDebugHitView, SemanticDebugView, SemanticModelStatusView,
};
use crate::docmind::search::normalize_query;
use crate::docmind::storage::types::{ChunkRecord, ExtractedDocument};
use crate::docmind::storage::Database;

use super::embedding::{
    cosine_similarity, embed_text, normalize_embedding_text, text_hash, vector_norm,
};

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

pub async fn get_embedding_model_status(
    database: &Database,
) -> Result<SemanticModelStatusView, String> {
    let model = load_default_model(database).await?;
    let sqlite_chunks = count_sqlite_chunks(database).await?;
    let embedded_chunks = count_embedded_chunks(database, &model.id).await?;
    let meta = load_vector_meta(database).await?;

    Ok(SemanticModelStatusView {
        needs_rebuild: sqlite_chunks != embedded_chunks,
        sqlite_chunks,
        embedded_chunks,
        last_indexed_at: format_unix_ts(meta.last_indexed_at),
        last_error: meta.last_error,
        index_status: meta.status,
        model,
    })
}

pub async fn list_embedding_models(
    database: &Database,
) -> Result<Vec<EmbeddingModelView>, String> {
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

    get_embedding_model_status(database).await
}

pub async fn rebuild_all_embeddings(database: &Database) -> Result<SemanticModelStatusView, String> {
    let model = load_default_model(database).await?;
    clear_all_embeddings(database).await?;

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

    let mut current_document_id = String::new();
    let mut document_chunks: Vec<(String, String, String, String)> = Vec::new();

    for row in rows {
        let document_id: String = row.try_get("document_id").map_err(|error| error.to_string())?;
        let file_name: String = row.try_get("file_name").map_err(|error| error.to_string())?;
        let chunk_id: String = row.try_get("chunk_id").map_err(|error| error.to_string())?;
        let heading: String = row.try_get("heading").unwrap_or_default();
        let snippet: String = row.try_get("snippet").unwrap_or_default();

        if current_document_id.is_empty() {
            current_document_id = document_id.clone();
        }

        if current_document_id != document_id {
            upsert_document_embeddings_from_rows(
                database,
                &current_document_id,
                &document_chunks,
                &model,
            )
            .await?;
            current_document_id = document_id.clone();
            document_chunks.clear();
        }

        document_chunks.push((chunk_id, file_name, heading, snippet));
    }

    if !current_document_id.is_empty() {
        upsert_document_embeddings_from_rows(
            database,
            &current_document_id,
            &document_chunks,
            &model,
        )
        .await?;
    }

    update_vector_index_meta(database, &model.id, "ready", "").await?;
    get_embedding_model_status(database).await
}

pub async fn upsert_document_embeddings(
    database: &Database,
    document_id: &str,
    document: &ExtractedDocument,
    chunks: &[ChunkRecord],
) -> Result<(), String> {
    let model = load_default_model(database).await?;
    if !model.enabled {
        return Ok(());
    }

    sqlx::query("DELETE FROM chunk_embeddings WHERE document_id = ?")
        .bind(document_id)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;

    for (index, chunk) in chunks.iter().enumerate() {
        let chunk_id = format!("{document_id}:{index}");
        let semantic_text = normalize_embedding_text(&format!(
            "{}\n{}\n{}",
            document.file_name, chunk.heading, chunk.snippet
        ));
        let vector = embed_text(&semantic_text, model.dimension);
        let vector_json = serde_json::to_string(&vector).map_err(|error| error.to_string())?;
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
        .bind(text_hash(&semantic_text))
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;
    }

    update_vector_index_meta(database, &model.id, "ready", "").await?;
    Ok(())
}

pub async fn delete_document_embeddings(database: &Database, path: &str) -> Result<(), String> {
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

pub async fn delete_directory_embeddings(database: &Database, dir_path: &str) -> Result<(), String> {
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

pub async fn clear_all_embeddings(database: &Database) -> Result<(), String> {
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

pub async fn semantic_debug_report(
    database: &Database,
    query: &str,
    limit: usize,
) -> Result<SemanticDebugView, String> {
    let model = load_default_model(database).await?;
    let normalized_query = normalize_query(query).join(" ");
    let query_vector = embed_text(&normalized_query, model.dimension);
    let query_vector_dim = query_vector.len();
    let query_vector_norm = vector_norm(&query_vector);
    let query_vector_ready = !normalized_query.trim().is_empty() && query_vector_norm > 0.0;

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
                heading: row.heading,
                snippet: row.snippet,
                paragraph: row.paragraph.map(|value| value as u32),
                page: row.page.map(|value| value as u32),
                score,
            });
        }

        hits.sort_by(|left, right| right.score.partial_cmp(&left.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(limit.max(1));
    }

    Ok(SemanticDebugView {
        query: query.to_string(),
        normalized_query,
        query_vector_dim,
        query_vector_ready,
        query_vector_norm,
        model,
        sqlite_chunks,
        embedded_chunks,
        hit_count: hits.len(),
        hits,
        index_status: meta.status,
        last_error: meta.last_error,
    })
}

async fn upsert_document_embeddings_from_rows(
    database: &Database,
    document_id: &str,
    chunks: &[(String, String, String, String)],
    model: &EmbeddingModelView,
) -> Result<(), String> {
    sqlx::query("DELETE FROM chunk_embeddings WHERE document_id = ?")
        .bind(document_id)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;

    for (chunk_id, file_name, heading, snippet) in chunks {
        let semantic_text = normalize_embedding_text(&format!("{file_name}\n{heading}\n{snippet}"));
        let vector = embed_text(&semantic_text, model.dimension);
        let vector_json = serde_json::to_string(&vector).map_err(|error| error.to_string())?;
        let now = now_unix_ts();

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO chunk_embeddings
                (chunk_id, document_id, model_id, vector_json, dimension, text_hash, status, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, 'ready', ?, ?)
            "#,
        )
        .bind(chunk_id)
        .bind(document_id)
        .bind(&model.id)
        .bind(vector_json)
        .bind(model.dimension as i64)
        .bind(text_hash(&semantic_text))
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .map_err(|error| error.to_string())?;
    }

    Ok(())
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
        SELECT model_id, chunk_count, last_indexed_at, last_error, status
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
    }))
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
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM chunk_embeddings
        WHERE model_id = ?
        "#,
    )
    .bind(model_id)
    .fetch_one(database.pool())
    .await
    .map_err(|error| error.to_string())?;
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
