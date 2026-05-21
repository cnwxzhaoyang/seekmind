use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub dir_path: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ExtractedDocument {
    pub dir_path: String,
    pub path: String,
    pub file_name: String,
    pub ext: String,
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
