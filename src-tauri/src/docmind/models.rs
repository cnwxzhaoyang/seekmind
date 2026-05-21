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
pub struct FailedFileView {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentTaskView {
    pub label: String,
    pub details: String,
    pub progress: u8,
    pub scanned: usize,
    pub total: usize,
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
