/**
 * @author MorningSun
 * @CreatedDate 2026/06/03
 * @Description SeekMind 本地 SQLite 存储与查询实现。
 */
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::seekmind::models::{
    HighlightSpan, QaHistoryView, QaMessageView, QaModelProfileView, QaRetrievalView,
    QaSessionView, QaSourceView,
};
use crate::seekmind::storage::fulltext::SearchIndex;
use crate::seekmind::storage::types::{NetworkProxySettings, QaSettings};

mod rows;
mod dirs;
mod collections;
mod qa;
mod tags;
mod search;
mod documents;
mod task;
mod settings;
mod schema;
mod util;

use self::rows::*;
use self::util::format_unix_ts;
pub use self::util::sqlite_database_path;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
    search_index: Arc<SearchIndex>,
    index_job_running: Arc<AtomicBool>,
}

impl Database {
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub(crate) fn try_begin_index_job(&self) -> bool {
        self.index_job_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(crate) fn end_index_job(&self) {
        self.index_job_running.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
struct DirectoryAggregate {
    path: String,
    enabled: bool,
    docs: usize,
    chunks: usize,
    status: String,
    is_explicit: bool,
}

impl Database {
    pub async fn open_or_init() -> Result<Self, String> {
        Self::open_or_init_with_options(false).await
    }

    pub async fn open_or_init_read_only_index() -> Result<Self, String> {
        Self::open_or_init_with_options(true).await
    }

    async fn open_or_init_with_options(read_only_index: bool) -> Result<Self, String> {
        let path = sqlite_database_path();
        eprintln!(
            "[SeekMind] SQLite database path: {} read_only_index={read_only_index}",
            path.display()
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .map_err(|error| error.to_string())?;

        let search_index = Arc::new(if read_only_index {
            SearchIndex::open_or_init_read_only()?
        } else {
            SearchIndex::open_or_init()?
        });
        let database = Self {
            pool,
            search_index,
            index_job_running: Arc::new(AtomicBool::new(false)),
        };
        database
            .init_schema()
            .await
            .map_err(|error| error.to_string())?;
        let documents_migrated = database
            .ensure_documents_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_current_task_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_failed_files_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_index_settings_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_index_settings_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_index_run_summary_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_index_checkpoint_table()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_embedding_models_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_vector_index_meta_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_vector_index_meta_row()
            .await
            .map_err(|error| error.to_string())?;
        if database
            .ensure_vector_index_schema_version()
            .await
            .map_err(|error| error.to_string())?
        {
            eprintln!("[SeekMind] vector index schema migration completed");
        }
        database
            .ensure_history_tables()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_qa_settings_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_qa_history_columns()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_network_proxy_settings_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_collections_seed()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_qa_model_profiles_row()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_chunks_block_indexes_column()
            .await
            .map_err(|error| error.to_string())?;
        database
            .ensure_document_blocks_columns()
            .await
            .map_err(|error| error.to_string())?;

        if documents_migrated {
            database
                .clear_all_index_data()
                .await
                .map_err(|error| error.to_string())?;
        }

        Ok(database)
    }
}

fn default_qa_settings() -> QaSettings {
    QaSettings {
        enabled: false,
        provider: "openai_compatible".to_string(),
        base_url: String::new(),
        api_key: String::new(),
        model: String::new(),
        temperature: 0.2,
        max_output_tokens: 600,
        context_chunk_limit: 8,
        context_token_budget: 6000,
        min_evidence_count: 1,
        min_retrieval_score: 0.0,
    }
}

fn default_network_proxy_settings() -> NetworkProxySettings {
    NetworkProxySettings {
        enabled: false,
        proxy_url: String::new(),
    }
}

fn qa_history_row_to_view(row: QaHistoryRow) -> QaHistoryView {
    let sources = serde_json::from_str::<Vec<QaSourceView>>(&row.sources_json).unwrap_or_default();
    let retrieval =
        serde_json::from_str::<QaRetrievalView>(&row.retrieval_json).unwrap_or(QaRetrievalView {
            search_mode: String::new(),
            candidate_count: 0,
            selected_count: 0,
            semantic_enabled: false,
            semantic_fallback: false,
            semantic_fallback_reason: String::new(),
        });

    QaHistoryView {
        id: row.id,
        question: row.question,
        answer: row.answer,
        state: row.state,
        sources,
        retrieval,
        model: row.model,
        created_at: format_unix_ts(row.created_at),
        error: {
            let trimmed = row.error.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
        warning: {
            let trimmed = row.warning.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
    }
}

fn qa_session_row_to_view(row: QaSessionRow) -> QaSessionView {
    QaSessionView {
        id: row.id,
        title: row.title,
        message_count: row.message_count.max(0) as usize,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn qa_message_row_to_view(row: QaMessageRow) -> QaMessageView {
    let sources = serde_json::from_str::<Vec<QaSourceView>>(&row.sources_json).unwrap_or_default();
    let retrieval =
        serde_json::from_str::<QaRetrievalView>(&row.retrieval_json).unwrap_or(QaRetrievalView {
            search_mode: String::new(),
            candidate_count: 0,
            selected_count: 0,
            semantic_enabled: false,
            semantic_fallback: false,
            semantic_fallback_reason: String::new(),
        });

    QaMessageView {
        id: row.id,
        session_id: row.session_id,
        question: row.question,
        answer: row.answer,
        state: row.state,
        sources,
        retrieval,
        model: row.model,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
        error: {
            let trimmed = row.error.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
        warning: {
            let trimmed = row.warning.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        },
    }
}

fn qa_model_profile_row_to_view(row: QaModelProfileRow) -> QaModelProfileView {
    QaModelProfileView {
        id: row.id,
        name: row.name,
        provider: row.provider,
        base_url: row.base_url,
        api_key: row.api_key,
        model: row.model,
        enabled: row.enabled != 0,
        is_default: row.is_default != 0,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn normalize_qa_session_title(title: &str) -> String {
    let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "新问答会话".to_string();
    }
    normalized.chars().take(40).collect()
}

fn matched_field_and_origin(row: &SearchRow, terms: &[String]) -> (String, String) {
    let candidates = [
        ("snippet", "正文摘要命中", row.snippet.as_str()),
        ("heading", "标题命中", row.heading.as_str()),
        ("filename", "文件名命中", row.file_name.as_str()),
        ("path", "路径命中", row.path.as_str()),
    ];

    for (field, origin, text) in candidates {
        let spans = highlight_spans(text, terms);
        if !spans.is_empty() {
            return (field.to_string(), origin.to_string());
        }
    }

    ("snippet".to_string(), "正文摘要命中".to_string())
}

fn highlight_spans(text: &str, terms: &[String]) -> Vec<HighlightSpan> {
    let lower_text = text.to_lowercase();
    let mut spans = Vec::new();

    for term in terms {
        let needle = term.trim();
        if needle.is_empty() {
            continue;
        }

        let normalized_needle = needle.to_lowercase();
        for (start, _) in lower_text.match_indices(&normalized_needle) {
            let start_char = lower_text[..start].chars().count();
            let end_char = start_char + normalized_needle.chars().count();
            spans.push(HighlightSpan {
                start: start_char,
                end: end_char,
            });
        }
    }

    if spans.is_empty() {
        return spans;
    }

    spans.sort_by_key(|span| (span.start, span.end));
    let mut merged: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.start <= last.end {
                last.end = last.end.max(span.end);
                continue;
            }
        }
        merged.push(span);
    }

    merged
}

fn build_search_snippet(
    text: &str,
    terms: &[String],
    max_chars: usize,
) -> (String, Vec<HighlightSpan>, usize, usize, usize) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (String::new(), Vec::new(), 0, 0, 0);
    }

    let spans = highlight_spans(trimmed, terms);
    if spans.is_empty() {
        let total = trimmed.chars().count();
        let snippet = truncate_by_chars(trimmed, max_chars);
        let end = total.min(max_chars);
        return (snippet, Vec::new(), 0, end, total);
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let total_chars = chars.len();
    if total_chars <= max_chars {
        return (trimmed.to_string(), spans, 0, total_chars, total_chars);
    }

    let focus_start = spans.first().map(|span| span.start).unwrap_or(0);
    let focus_end = spans.last().map(|span| span.end).unwrap_or(total_chars);
    let target = max_chars.min(total_chars);
    let before = target.saturating_mul(2) / 5;
    let after = target.saturating_sub(before);

    let mut snippet_start = focus_start.saturating_sub(before);
    let mut snippet_end = (focus_end + after).min(total_chars);

    if snippet_end.saturating_sub(snippet_start) < target {
        let shortfall = target - (snippet_end - snippet_start);
        let left_pad = shortfall / 2;
        let right_pad = shortfall - left_pad;
        snippet_start = snippet_start.saturating_sub(left_pad);
        snippet_end = (snippet_end + right_pad).min(total_chars);
    }

    if snippet_end.saturating_sub(snippet_start) > target {
        snippet_end = snippet_start + target;
    }

    let leading_ellipsis = snippet_start > 0;
    let trailing_ellipsis = snippet_end < total_chars;
    let mut snippet = slice_chars(trimmed, snippet_start, snippet_end);
    if leading_ellipsis {
        snippet = format!("…{snippet}");
    }
    if trailing_ellipsis {
        snippet.push('…');
    }

    let prefix_offset = if leading_ellipsis { 1 } else { 0 };
    let mut adjusted_spans = Vec::with_capacity(spans.len());
    for span in spans {
        let start = span.start.max(snippet_start);
        let end = span.end.min(snippet_end);
        if end <= start {
            continue;
        }
        adjusted_spans.push(HighlightSpan {
            start: start - snippet_start + prefix_offset,
            end: end - snippet_start + prefix_offset,
        });
    }

    (
        snippet,
        adjusted_spans,
        snippet_start,
        snippet_end,
        total_chars,
    )
}

fn boosted_search_score(base_score: f32, match_origin: &str, modified_at: i64, now: i64) -> f32 {
    let field_boost = match match_origin {
        "文件名命中" => 0.45,
        "标题命中" => 0.32,
        "正文摘要命中" => 0.18,
        "路径命中" => 0.08,
        _ => 0.12,
    };

    let recency_boost = if modified_at > 0 && now > modified_at {
        let age_days = ((now - modified_at) as f32 / 86_400.0).min(3_650.0);
        (1.0 / (1.0 + age_days / 30.0)) * 0.15
    } else {
        0.05
    };

    base_score + field_boost + recency_boost
}

fn rerank_bonus(is_favorite: bool, recent_open_count: usize, history_expanded: bool) -> f32 {
    let mut bonus = 0.0;

    if is_favorite {
        bonus += 0.12;
    }

    if recent_open_count > 0 {
        bonus += 0.04;
        if recent_open_count >= 3 {
            bonus += 0.02;
        }
    }

    if history_expanded {
        bonus += 0.03;
    }

    bonus
}

fn search_rank_reason(
    row: &SearchRow,
    terms: &[String],
    match_origin: &str,
    keyword_score: f32,
    semantic_score: f32,
    semantic_enabled: bool,
    is_favorite: bool,
    recent_open_count: usize,
    history_expanded: bool,
    title_score: f32,
    filename_score: f32,
    preference_score: f32,
    modified_at: i64,
    now: i64,
) -> crate::seekmind::models::SearchRankReasonView {
    let mut boosts = vec![match_origin.to_string()];

    if contains_all_terms(&row.file_name, terms) {
        boosts.push("文件名完整命中".to_string());
    } else if contains_all_terms(&row.heading, terms) {
        boosts.push("标题完整命中".to_string());
    } else if contains_all_terms(&row.path, terms) {
        boosts.push("路径完整命中".to_string());
    }

    if semantic_enabled {
        if keyword_score > 0.0 && semantic_score > 0.0 {
            boosts.push("全文+语义共同命中".to_string());
        } else if semantic_score > 0.0 {
            boosts.push("语义召回".to_string());
        }
    }

    if is_favorite {
        boosts.push("已收藏优先".to_string());
    }

    if recent_open_count > 0 {
        if recent_open_count >= 3 {
            boosts.push(format!("最近打开 {recent_open_count} 次"));
        } else {
            boosts.push("最近打开".to_string());
        }
    }

    if history_expanded {
        boosts.push("历史扩展".to_string());
    }

    if modified_at > 0 && now > modified_at {
        let age_days = ((now - modified_at) as f32 / 86_400.0).min(3_650.0);
        if age_days < 30.0 {
            boosts.push("最近更新".to_string());
        }
    }

    let boosts = dedupe_reason_parts(boosts);
    crate::seekmind::models::SearchRankReasonView {
        summary: boosts.join(" · "),
        match_origin: match_origin.to_string(),
        boosts,
        keyword_hit: keyword_score > 0.0,
        semantic_hit: semantic_score > 0.0,
        favorite_boost: is_favorite,
        recent_open_count,
        history_expanded,
        keyword_score: keyword_score.max(0.0),
        semantic_score: semantic_score.max(0.0),
        title_score,
        filename_score,
        preference_score,
        base_score: 0.0,
        raw_score: 0.0,
        original_rank: 0,
        final_rank: 0,
        rank_delta: 0,
    }
}

fn dedupe_reason_parts(parts: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for part in parts {
        if part.trim().is_empty() {
            continue;
        }
        if !deduped.iter().any(|existing| existing == &part) {
            deduped.push(part);
        }
    }
    deduped
}

fn contains_all_terms(text: &str, terms: &[String]) -> bool {
    let lower = text.to_lowercase();
    let filtered_terms: Vec<String> = terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();

    !filtered_terms.is_empty() && filtered_terms.iter().all(|term| lower.contains(term))
}

fn contains_any_term(text: &str, terms: &[String]) -> bool {
    let lower = text.to_lowercase();
    let filtered_terms: Vec<String> = terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();

    !filtered_terms.is_empty() && filtered_terms.iter().any(|term| lower.contains(term))
}

fn truncate_by_chars(text: &str, limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        return text.to_string();
    }

    let mut snippet: String = chars.iter().take(limit).collect();
    snippet.push('…');
    snippet
}

fn slice_chars(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}
