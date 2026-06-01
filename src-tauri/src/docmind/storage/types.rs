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
    pub block_indexes: Vec<usize>,
}

#[derive(Debug, Clone)]
pub enum ParserSource {
    Python,
    Rust,
}

#[derive(Debug, Clone)]
pub struct ParseOutcome {
    pub source: ParserSource,
    pub warning: Option<String>,
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

#[derive(Debug, Clone)]
pub struct QaSettings {
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
}

#[derive(Debug, Clone)]
pub struct NetworkProxySettings {
    pub enabled: bool,
    pub proxy_url: String,
}
