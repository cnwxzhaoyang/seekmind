/**
 * @author MorningSun
 * @CreatedDate 2026/06/03
 * @Description 问答上下文检索与来源构建。
 */
use std::collections::{HashMap, HashSet};

use crate::docmind::models::{QaRetrievalView, QaSourceView};
use crate::docmind::storage::db::Database;
use crate::docmind::storage::types::QaSettings;

use super::models::{QaContext, QaSourceBlock};

fn matches_scope(path: &str, scope_paths: &[String]) -> bool {
    if scope_paths.is_empty() {
        return true;
    }

    scope_paths.iter().any(|scope| {
        let trimmed = scope.trim();
        !trimmed.is_empty() && path.starts_with(trimmed)
    })
}

fn build_location_label(source: &crate::docmind::models::SearchResultView) -> String {
    if !source.title_path.trim().is_empty() {
        return source.title_path.clone();
    }

    if !source.heading.trim().is_empty() {
        return source.heading.clone();
    }

    source.file_name.clone()
}

fn build_prompt_block(
    source_id: &str,
    file_name: &str,
    path: &str,
    location: &str,
    previous: Option<&str>,
    current: &str,
    next: Option<&str>,
) -> String {
    let mut lines = vec![
        format!("[{source_id}]"),
        format!("文件: {file_name}"),
        format!("路径: {path}"),
        format!("位置: {location}"),
        "上下文:".to_string(),
    ];

    if let Some(previous) = previous.filter(|text| !text.trim().is_empty()) {
        lines.push(format!("- 上一段: {}", previous.trim()));
    }

    lines.push(format!("- 当前段: {}", current.trim()));

    if let Some(next) = next.filter(|text| !text.trim().is_empty()) {
        lines.push(format!("- 下一段: {}", next.trim()));
    }

    lines.join("\n")
}

fn source_term_overlap(
    source: &crate::docmind::models::SearchResultView,
    terms: &[String],
) -> usize {
    if terms.is_empty() {
        return 0;
    }

    let title_haystack = format!(
        "{} {} {}",
        source.file_name, source.title_path, source.heading
    )
    .to_lowercase();
    let body_haystack = format!(
        "{} {} {} {} {}",
        source.file_name, source.title_path, source.heading, source.path, source.snippet
    )
    .to_lowercase();

    terms
        .iter()
        .map(|term| {
            let term = term.trim().to_lowercase();
            if term.len() < 2 {
                0
            } else if title_haystack.contains(&term) {
                3
            } else if body_haystack.contains(&term) {
                1
            } else {
                0
            }
        })
        .sum()
}

pub async fn build_qa_context(
    database: &Database,
    question: &str,
    scope_paths: &[String],
    settings: &QaSettings,
    limit: usize,
    session_terms: &[String],
) -> Result<QaContext, String> {
    let recall_limit = limit.max(1).saturating_mul(3).max(20);
    let mut augmented_query = question.trim().to_string();
    if !session_terms.is_empty() {
        augmented_query.push(' ');
        augmented_query.push_str(&session_terms.join(" "));
    }

    let debug = database
        .search_documents_debug(&augmented_query, recall_limit)
        .await
        .map_err(|error| error.to_string())?;

    let search_mode = debug.search_mode;
    let semantic_enabled = debug.semantic_enabled;
    let semantic_fallback = debug.semantic_fallback;
    let semantic_fallback_reason = debug.semantic_fallback_reason;
    let mut hits = debug.hits;
    if !session_terms.is_empty() {
        hits.sort_by(|left, right| {
            let right_overlap = source_term_overlap(right, session_terms);
            let left_overlap = source_term_overlap(left, session_terms);
            right_overlap
                .cmp(&left_overlap)
                .then_with(|| right.score.total_cmp(&left.score))
        });
    }

    let mut seen_chunk_ids = HashSet::<String>::new();
    let mut selected_hits = Vec::new();
    let candidate_count = hits.len();
    for hit in hits {
        if hit.score < settings.min_retrieval_score {
            continue;
        }
        if !matches_scope(&hit.path, scope_paths) {
            continue;
        }
        if seen_chunk_ids.insert(hit.id.clone()) {
            selected_hits.push(hit);
        }
        if selected_hits.len() >= settings.context_chunk_limit {
            break;
        }
    }

    let mut sources = Vec::<QaSourceBlock>::new();
    let mut loaded_chunks = HashMap::<String, Vec<crate::docmind::models::ChunkView>>::new();

    for (index, hit) in selected_hits.into_iter().enumerate() {
        let chunks = if let Some(chunks) = loaded_chunks.get(&hit.path) {
            chunks.clone()
        } else {
            let chunks = database
                .list_document_chunks(&hit.path)
                .await
                .map_err(|error| error.to_string())?;
            loaded_chunks.insert(hit.path.clone(), chunks.clone());
            chunks
        };

        let matched_index = chunks.iter().position(|chunk| chunk.id == hit.id);
        let (previous, current, next) = if let Some(position) = matched_index {
            let previous = chunks
                .get(position.saturating_sub(1))
                .map(|chunk| chunk.snippet.as_str());
            let current = chunks
                .get(position)
                .map(|chunk| chunk.snippet.as_str())
                .unwrap_or(hit.snippet.as_str());
            let next = chunks.get(position + 1).map(|chunk| chunk.snippet.as_str());
            (previous, current, next)
        } else {
            (None, hit.snippet.as_str(), None)
        };

        let source_id = format!("S{}", index + 1);
        let location_label = build_location_label(&hit);
        let block = build_prompt_block(
            &source_id,
            &hit.file_name,
            &hit.path,
            &location_label,
            previous,
            current,
            next,
        );

        sources.push(QaSourceBlock {
            source: QaSourceView {
                source_id,
                chunk_id: hit.id,
                file_name: hit.file_name,
                path: hit.path,
                ext: hit.ext,
                title_path: hit.title_path,
                heading: hit.heading,
                paragraph: hit.paragraph,
                page: hit.page,
                snippet: hit.snippet,
                score: hit.score,
                rank_reason: hit.rank_reason.summary,
                preview_blocks: hit.preview_blocks,
            },
            block,
        });
    }

    let retrieval = QaRetrievalView {
        search_mode,
        candidate_count,
        selected_count: sources.len(),
        semantic_enabled,
        semantic_fallback,
        semantic_fallback_reason,
    };

    Ok(QaContext { sources, retrieval })
}
