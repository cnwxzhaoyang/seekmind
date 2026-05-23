use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct IndexDirView {
    pub path: String,
    pub enabled: bool,
    pub docs: usize,
    pub chunks: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultView {
    pub id: String,
    pub file_name: String,
    pub path: String,
    pub ext: String,
    pub heading: String,
    pub snippet: String,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentTaskView {
    pub label: String,
    pub details: String,
    pub current_dir: String,
    pub current_file: String,
    pub progress: u8,
    pub scanned: usize,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexStatusView {
    pub indexed_docs: usize,
    pub indexed_chunks: usize,
    pub scanned_docs: usize,
    pub failed_files: usize,
    pub current_task: Option<CurrentTaskView>,
    pub failed_items: Vec<FailedFileView>,
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
