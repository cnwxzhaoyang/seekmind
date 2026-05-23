use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct IndexDirView {
    pub path: String,
    pub enabled: bool,
    pub docs: usize,
    pub chunks: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultView {
    pub id: String,
    pub file_name: String,
    pub path: String,
    pub ext: String,
    pub heading: String,
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
pub struct ChunkView {
    pub id: String,
    pub heading: String,
    pub snippet: String,
    pub paragraph: Option<u32>,
    pub page: Option<u32>,
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
pub struct ParserRuntimeView {
    pub enabled: bool,
    pub available: bool,
    pub active: String,
    pub python_bin: String,
    pub script_path: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchDebugView {
    pub query: String,
    pub normalized_terms: Vec<String>,
    pub normalized_search_text: String,
    pub sqlite_documents: usize,
    pub sqlite_chunks: usize,
    pub tantivy_documents: usize,
    pub hit_count: usize,
    pub hits: Vec<SearchResultView>,
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
}
