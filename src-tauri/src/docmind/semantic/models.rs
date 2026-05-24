#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct SemanticEmbeddingModelRow {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model_path: String,
    pub dimension: usize,
    pub enabled: bool,
    pub available: bool,
    pub is_default: bool,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticModelStatusView {
    pub model: SemanticEmbeddingModelRow,
    pub sqlite_chunks: usize,
    pub embedded_chunks: usize,
    pub needs_rebuild: bool,
    pub last_indexed_at: String,
    pub last_error: String,
    pub index_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticDebugHitView {
    pub chunk_id: String,
    pub document_path: String,
    pub file_name: String,
    pub heading: String,
    pub snippet: String,
    pub paragraph: Option<u32>,
    pub page: Option<u32>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticDebugView {
    pub query: String,
    pub normalized_query: String,
    pub rewritten_query: String,
    pub rewritten_terms: Vec<String>,
    pub query_rewrite_applied: bool,
    pub query_vector_dim: usize,
    pub query_vector_ready: bool,
    pub query_vector_norm: f32,
    pub model: SemanticEmbeddingModelRow,
    pub sqlite_chunks: usize,
    pub embedded_chunks: usize,
    pub hit_count: usize,
    pub semantic_threshold: f32,
    pub semantic_candidate_count: usize,
    pub semantic_filtered_count: usize,
    pub hits: Vec<SemanticDebugHitView>,
    pub index_status: String,
    pub last_error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticEmbeddingVector {
    pub chunk_id: String,
    pub document_id: String,
    pub model_id: String,
    pub vector_json: String,
    pub dimension: usize,
    pub text_hash: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}
