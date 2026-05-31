use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct IndexDirView {
    pub path: String,
    pub enabled: bool,
    pub docs: usize,
    pub chunks: usize,
    pub status: String,
    pub is_explicit: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedPathView {
    pub path: String,
    pub dir_path: String,
    pub is_virtual: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPathsView {
    pub added_dirs: Vec<String>,
    pub imported_files: Vec<ImportedPathView>,
    pub virtual_dir: String,
    pub skipped: Vec<String>,
    pub unsupported: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchRankReasonView {
    pub summary: String,
    pub match_origin: String,
    pub boosts: Vec<String>,
    pub keyword_hit: bool,
    pub semantic_hit: bool,
    pub favorite_boost: bool,
    pub recent_open_count: usize,
    pub history_expanded: bool,
    pub keyword_score: f32,
    pub semantic_score: f32,
    pub title_score: f32,
    pub filename_score: f32,
    pub preference_score: f32,
    pub base_score: f32,
    pub raw_score: f32,
    pub original_rank: usize,
    pub final_rank: usize,
    pub rank_delta: isize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultView {
    pub id: String,
    pub file_name: String,
    pub path: String,
    pub ext: String,
    pub heading: String,
    pub title_path: String,
    pub snippet: String,
    pub matched_field: String,
    pub match_origin: String,
    pub highlight_spans: Vec<HighlightSpan>,
    pub snippet_window_start: usize,
    pub snippet_window_end: usize,
    pub snippet_source_len: usize,
    pub paragraph: Option<u32>,
    pub page: Option<u32>,
    pub modified: String,
    pub score: f32,
    pub rank_reason: SearchRankReasonView,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentView {
    pub id: String,
    pub dir_path: String,
    pub path: String,
    pub file_name: String,
    pub ext: String,
    pub modified: String,
    pub chunks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewBlockView {
    pub block_index: usize,
    pub block_type: String,
    pub text: String,
    pub heading: String,
    pub level: Option<u32>,
    pub page: Option<u32>,
    pub language: String,
    pub markdown: String,
    pub html: String,
    #[serde(default)]
    pub asset_path: String,
    #[serde(default)]
    pub alt_text: String,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub ocr_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChunkView {
    pub id: String,
    pub heading: String,
    pub title_path: String,
    pub snippet: String,
    pub paragraph: Option<u32>,
    pub page: Option<u32>,
    #[serde(default)]
    pub preview_blocks: Vec<PreviewBlockView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailedFileView {
    pub file: String,
    pub reason: String,
    pub category: String,
    pub code: String,
    pub retry_count: usize,
    pub last_failed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentTaskView {
    pub label: String,
    pub details: String,
    pub state: String,
    pub current_dir: String,
    pub current_file: String,
    pub progress: u8,
    pub scanned: usize,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub updated: usize,
    pub skipped: usize,
    pub deleted: usize,
    #[serde(default)]
    pub warning: Option<String>,
    pub pause_requested: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexRunSummaryView {
    pub updated: usize,
    pub skipped: usize,
    pub deleted: usize,
    pub scanned: usize,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub completed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexStatusView {
    pub indexed_docs: usize,
    pub indexed_chunks: usize,
    pub scanned_docs: usize,
    pub failed_files: usize,
    pub current_task: Option<CurrentTaskView>,
    pub failed_items: Vec<FailedFileView>,
    pub last_run: Option<IndexRunSummaryView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentRefreshStartView {
    pub job_id: String,
    pub status: IndexStatusView,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentRefreshProgressView {
    pub job_id: String,
    pub state: String,
    pub message: String,
    pub path: String,
    pub file_name: String,
    pub parser_source: String,
    pub warning: Option<String>,
    pub status: IndexStatusView,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexRefreshStartView {
    pub job_id: String,
    pub status: IndexStatusView,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexRefreshProgressView {
    pub job_id: String,
    pub state: String,
    pub message: String,
    pub scope: String,
    pub path: String,
    pub status: IndexStatusView,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParserRuntimeView {
    pub enabled: bool,
    pub available: bool,
    pub active: String,
    pub system_locale: String,
    pub system_language: String,
    pub tesseract_languages: Vec<String>,
    pub chinese_ocr_available: bool,
    #[serde(default)]
    pub chinese_ocr_warning: Option<String>,
    pub python_bin: String,
    pub script_path: String,
    pub timeout_ms: u64,
    pub office_enabled: bool,
    pub office_available: bool,
    pub office_bin: String,
    pub office_message: String,
    pub office_platform: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchDebugView {
    pub query: String,
    pub normalized_terms: Vec<String>,
    pub normalized_search_text: String,
    pub rewritten_query: String,
    pub rewritten_terms: Vec<String>,
    pub query_rewrite_applied: bool,
    pub history_terms: Vec<String>,
    pub history_rewrite_applied: bool,
    pub expanded_query: String,
    pub sqlite_documents: usize,
    pub sqlite_chunks: usize,
    pub tantivy_documents: usize,
    pub semantic_enabled: bool,
    pub semantic_weight: f32,
    pub semantic_threshold: f32,
    pub keyword_hit_count: usize,
    pub semantic_hit_count: usize,
    pub semantic_candidate_count: usize,
    pub semantic_filtered_count: usize,
    pub semantic_fallback: bool,
    pub semantic_fallback_reason: String,
    pub search_mode: String,
    pub hit_count: usize,
    pub hits: Vec<SearchResultView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchDebugReportEventView {
    pub request_id: String,
    pub state: String,
    pub query: String,
    pub report: Option<SearchDebugView>,
    pub error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHistoryView {
    pub query: String,
    pub normalized_query: String,
    pub hit_count: usize,
    pub last_hit_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentDocumentView {
    pub path: String,
    pub title: String,
    pub file_name: String,
    pub ext: String,
    pub last_opened_at: String,
    pub open_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FavoriteView {
    pub favorite_type: String,
    pub target: String,
    pub title: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexSettingsView {
    pub exclude_dirs: Vec<String>,
    pub exclude_exts: Vec<String>,
    pub max_file_size_mb: u64,
    pub semantic_search_enabled: bool,
    pub semantic_weight: f32,
    pub semantic_threshold: f32,
    pub title_weight: f32,
    pub filename_weight: f32,
    pub preference_weight: f32,
    pub prefer_favorites_enabled: bool,
    pub prefer_recent_enabled: bool,
    pub prefer_history_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSettingsView {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_output_tokens: usize,
    pub context_chunk_limit: usize,
    pub context_token_budget: usize,
    pub min_evidence_count: usize,
    pub min_retrieval_score: f32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSourceView {
    pub source_id: String,
    pub chunk_id: String,
    pub file_name: String,
    pub path: String,
    pub ext: String,
    pub title_path: String,
    pub heading: String,
    pub paragraph: Option<u32>,
    pub page: Option<u32>,
    pub snippet: String,
    pub score: f32,
    pub rank_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaRetrievalView {
    pub search_mode: String,
    pub candidate_count: usize,
    pub selected_count: usize,
    pub semantic_enabled: bool,
    pub semantic_fallback: bool,
    pub semantic_fallback_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaAnswerView {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub state: String,
    pub sources: Vec<QaSourceView>,
    pub retrieval: QaRetrievalView,
    pub model: String,
    pub created_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaHistoryView {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub state: String,
    pub sources: Vec<QaSourceView>,
    pub retrieval: QaRetrievalView,
    pub model: String,
    pub created_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSessionView {
    pub id: String,
    pub title: String,
    pub message_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaMessageView {
    pub id: String,
    pub session_id: String,
    pub question: String,
    pub answer: String,
    pub state: String,
    pub sources: Vec<QaSourceView>,
    pub retrieval: QaRetrievalView,
    pub model: String,
    pub created_at: String,
    pub updated_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaConnectionTestView {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaAskStartView {
    pub job_id: String,
    pub status: QaAnswerView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaAnswerProgressView {
    pub job_id: String,
    pub state: String,
    pub question: String,
    pub answer: String,
    pub sources: Vec<QaSourceView>,
    pub retrieval: QaRetrievalView,
    pub model: String,
    pub error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingModelView {
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
    pub model: EmbeddingModelView,
    pub sqlite_chunks: usize,
    pub embedded_chunks: usize,
    pub needs_rebuild: bool,
    pub last_indexed_at: String,
    pub last_error: String,
    pub index_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRebuildProgressView {
    pub job_id: String,
    pub state: String,
    pub message: String,
    pub source: String,
    pub model: EmbeddingModelView,
    pub total_chunks: usize,
    pub processed_chunks: usize,
    pub embedded_chunks: usize,
    pub current_document: String,
    pub current_chunk: String,
    pub percent: u8,
    pub last_error: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticRebuildStartView {
    pub job_id: String,
    pub status: SemanticModelStatusView,
}

#[derive(Debug, Clone, Serialize)]
pub struct SemanticDebugHitView {
    pub chunk_id: String,
    pub document_path: String,
    pub file_name: String,
    pub heading: String,
    pub title_path: String,
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
    pub model: EmbeddingModelView,
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
