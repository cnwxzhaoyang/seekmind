#![allow(dead_code)]

/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind SQLite 数据库行结构体定义。
 */

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct IndexDirRow {
    pub(crate) path: String,
    pub(crate) enabled: i64,
    pub(crate) docs: i64,
    pub(crate) chunks: i64,
    pub(crate) status: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct DocumentPathRow {
    pub(crate) path: String,
    pub(crate) dir_path: String,
    pub(crate) chunks: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct FulltextDocumentRow {
    pub(crate) id: String,
    pub(crate) dir_path: String,
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) ext: String,
    pub(crate) file_size: i64,
    pub(crate) modified_at: i64,
    pub(crate) content_hash: String,
    pub(crate) modified: String,
    pub(crate) content: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct FulltextChunkRow {
    pub(crate) heading: String,
    pub(crate) snippet: String,
    pub(crate) paragraph: Option<i64>,
    pub(crate) page: Option<i64>,
    pub(crate) score: f32,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SearchRow {
    pub(crate) id: String,
    pub(crate) document_id: String,
    pub(crate) file_name: String,
    pub(crate) path: String,
    pub(crate) ext: String,
    pub(crate) heading: String,
    pub(crate) snippet: String,
    pub(crate) paragraph: Option<i64>,
    pub(crate) page: Option<i64>,
    pub(crate) modified: String,
    pub(crate) modified_at: i64,
    pub(crate) score: f32,
    pub(crate) block_indexes_json: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct DocumentRow {
    pub(crate) id: String,
    pub(crate) dir_path: String,
    pub(crate) path: String,
    pub(crate) file_name: String,
    pub(crate) ext: String,
    pub(crate) modified: String,
    pub(crate) chunks: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ChunkRow {
    pub(crate) id: String,
    pub(crate) heading: String,
    pub(crate) snippet: String,
    pub(crate) paragraph: Option<i64>,
    pub(crate) page: Option<i64>,
    pub(crate) block_indexes_json: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct BlockRow {
    pub(crate) block_index: i64,
    pub(crate) block_type: String,
    pub(crate) text: String,
    pub(crate) heading: String,
    pub(crate) level: Option<i64>,
    pub(crate) page: Option<i64>,
    pub(crate) language: String,
    pub(crate) markdown: String,
    pub(crate) html: String,
    pub(crate) asset_path: String,
    pub(crate) alt_text: String,
    pub(crate) caption: String,
    pub(crate) ocr_text: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct BlockWithDocumentRow {
    pub(crate) document_id: String,
    pub(crate) block_index: i64,
    pub(crate) block_type: String,
    pub(crate) text: String,
    pub(crate) heading: String,
    pub(crate) level: Option<i64>,
    pub(crate) page: Option<i64>,
    pub(crate) language: String,
    pub(crate) markdown: String,
    pub(crate) html: String,
    pub(crate) asset_path: String,
    pub(crate) alt_text: String,
    pub(crate) caption: String,
    pub(crate) ocr_text: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct FailedFileRow {
    pub(crate) file: String,
    pub(crate) reason: String,
    pub(crate) category: String,
    pub(crate) code: String,
    pub(crate) retry_count: i64,
    pub(crate) last_failed_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct IndexRunSummaryRow {
    pub(crate) updated: i64,
    pub(crate) skipped: i64,
    pub(crate) deleted: i64,
    pub(crate) scanned: i64,
    pub(crate) total: i64,
    pub(crate) succeeded: i64,
    pub(crate) failed: i64,
    pub(crate) completed_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct SearchHistoryRow {
    pub(crate) query: String,
    pub(crate) normalized_query: String,
    pub(crate) hit_count: i64,
    pub(crate) last_hit_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct RecentDocumentRow {
    pub(crate) path: String,
    pub(crate) title: String,
    pub(crate) file_name: String,
    pub(crate) ext: String,
    pub(crate) last_opened_at: i64,
    pub(crate) open_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct RecentViewRow {
    pub(crate) target_type: String,
    pub(crate) target_id: String,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) viewed_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TagRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) target_count: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TargetTagRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct FavoriteRow {
    pub(crate) favorite_type: String,
    pub(crate) target: String,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct CollectionRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) color: String,
    pub(crate) sort_order: i64,
    pub(crate) item_count: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct CollectionItemRow {
    pub(crate) id: String,
    pub(crate) collection_id: String,
    pub(crate) item_type: String,
    pub(crate) document_id: String,
    pub(crate) chunk_id: String,
    pub(crate) qa_session_id: String,
    pub(crate) qa_message_id: String,
    pub(crate) title: String,
    pub(crate) path: String,
    pub(crate) title_path: String,
    pub(crate) snippet: String,
    pub(crate) note: String,
    pub(crate) source_meta_json: String,
    pub(crate) sort_order: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct QaSettingsRow {
    pub(crate) enabled: i64,
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) temperature: f32,
    pub(crate) max_output_tokens: i64,
    pub(crate) context_chunk_limit: i64,
    pub(crate) context_token_budget: i64,
    pub(crate) min_evidence_count: i64,
    pub(crate) min_retrieval_score: f32,
    pub(crate) intent_synonym_rules_json: String,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct NetworkProxySettingsRow {
    pub(crate) enabled: i64,
    pub(crate) proxy_url: String,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct QaModelProfileRow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) provider: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) enabled: i64,
    pub(crate) is_default: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct QaHistoryRow {
    pub(crate) id: String,
    pub(crate) question: String,
    pub(crate) answer: String,
    pub(crate) state: String,
    pub(crate) sources_json: String,
    pub(crate) retrieval_json: String,
    pub(crate) model: String,
    pub(crate) error: String,
    pub(crate) warning: String,
    pub(crate) created_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct QaSessionRow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) message_count: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct QaMessageRow {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) question: String,
    pub(crate) answer: String,
    pub(crate) state: String,
    pub(crate) sources_json: String,
    pub(crate) retrieval_json: String,
    pub(crate) model: String,
    pub(crate) error: String,
    pub(crate) warning: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct IndexCheckpointRow {
    pub(crate) dir_paths: String,
    pub(crate) pending_delete_paths: String,
    pub(crate) pending_update_paths: String,
    pub(crate) phase: String,
    pub(crate) current_dir: String,
    pub(crate) current_file: String,
    pub(crate) total: i64,
    pub(crate) processed: i64,
    pub(crate) succeeded: i64,
    pub(crate) failed: i64,
    pub(crate) updated: i64,
    pub(crate) skipped: i64,
    pub(crate) deleted: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct CurrentTaskRow {
    pub(crate) label: String,
    pub(crate) details: String,
    pub(crate) state: String,
    pub(crate) current_dir: String,
    pub(crate) current_file: String,
    pub(crate) started_at: i64,
    pub(crate) progress: i64,
    pub(crate) scanned: i64,
    pub(crate) total: i64,
    pub(crate) succeeded: i64,
    pub(crate) failed: i64,
    pub(crate) updated: i64,
    pub(crate) skipped: i64,
    pub(crate) deleted: i64,
    pub(crate) warning: Option<String>,
    pub(crate) pause_requested: i64,
}
