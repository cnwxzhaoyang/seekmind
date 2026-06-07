/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind Python 解析结果与流式消息类型。
 */
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedBlock {
    pub block_index: usize,
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
    pub heading: Option<String>,
    pub level: Option<u32>,
    pub page_no: Option<u32>,
    #[serde(default)]
    pub language: Option<String>,
    pub markdown: Option<String>,
    pub html: Option<String>,
    #[serde(default)]
    pub asset_path: Option<String>,
    #[serde(default)]
    pub alt_text: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub ocr_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfOcrTask {
    pub page_index: usize,
    pub reason: String,
    pub message: String,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub ocr_text: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub title: Option<String>,
    pub file_type: String,
    pub content: String,
    pub chunks: Vec<ParsedChunk>,
    #[serde(default)]
    pub blocks: Option<Vec<ParsedBlock>>,
    #[serde(default)]
    pub ocr_tasks: Option<Vec<PdfOcrTask>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedChunk {
    pub heading: Option<String>,
    pub page_no: Option<u32>,
    pub text: String,
    pub order: usize,
    #[serde(default = "default_chunk_score")]
    pub score: f32,
    #[serde(default)]
    pub block_indexes: Option<Vec<usize>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserOptions {
    pub include_chunks: bool,
    pub max_chunk_chars: usize,
    pub max_chunks: Option<usize>,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            include_chunks: true,
            max_chunk_chars: 800,
            max_chunks: None,
        }
    }
}

fn default_chunk_score() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserRequest {
    pub request_id: String,
    pub command: String,
    pub path: String,
    pub options: ParserOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserResponse {
    #[serde(default)]
    pub kind: Option<String>,
    pub request_id: String,
    pub ok: bool,
    pub document: Option<ParsedDocument>,
    pub error: Option<ParserError>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParserStreamEvent {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub percent: u8,
    #[serde(default)]
    pub current: String,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub processed: usize,
    #[serde(default)]
    pub parser_source: String,
    #[serde(default)]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserStreamMessage {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub percent: u8,
    #[serde(default)]
    pub current: String,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub processed: usize,
    #[serde(default)]
    pub parser_source: String,
    #[serde(default)]
    pub warning: Option<String>,
}
