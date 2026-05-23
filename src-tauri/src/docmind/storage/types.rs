use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub dir_path: String,
    pub path: PathBuf,
    pub file_size: i64,
    pub modified_at: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct ExtractedDocument {
    pub dir_path: String,
    pub path: String,
    pub file_name: String,
    pub ext: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub content_hash: String,
    pub modified: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ChunkRecord {
    pub heading: String,
    pub snippet: String,
    pub paragraph: Option<i64>,
    pub page: Option<i64>,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct DocumentState {
    pub path: String,
    pub file_size: i64,
    pub modified_at: i64,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct IndexSettings {
    pub exclude_dirs: Vec<String>,
    pub exclude_exts: Vec<String>,
    pub max_file_size_mb: u64,
}
