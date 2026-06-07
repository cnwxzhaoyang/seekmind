/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 本地 SQLite 检索、历史与结果排序实现。
 */
use std::collections::{HashMap, HashSet};

use crate::seekmind::models::{
    PreviewBlockView, RecentDocumentView, RecentViewEntry, SearchHistoryView, SearchResultView,
};
use crate::seekmind::search::{normalize_query, rewrite_query_terms, rewrite_search_text};
use crate::seekmind::semantic::store as semantic_store;
use crate::seekmind::storage::types::{ChunkRecord, ExtractedDocument};

use super::collections::favorite_result_target;
use super::rows::{
    BlockWithDocumentRow, FulltextChunkRow, FulltextDocumentRow, RecentDocumentRow, RecentViewRow,
    SearchHistoryRow, SearchRow,
};
use super::util::{current_unix_ts, format_unix_ts};
use super::Database;

#[derive(Debug, Clone)]
pub(crate) struct SearchDebugData {
    pub(crate) hits: Vec<SearchResultView>,
    pub(crate) keyword_hit_count: usize,
    pub(crate) semantic_hit_count: usize,
    pub(crate) semantic_candidate_count: usize,
    pub(crate) semantic_filtered_count: usize,
    pub(crate) semantic_enabled: bool,
    pub(crate) semantic_weight: f32,
    pub(crate) semantic_threshold: f32,
    pub(crate) rewritten_terms: Vec<String>,
    pub(crate) rewritten_query: String,
    pub(crate) history_terms: Vec<String>,
    pub(crate) history_rewrite_applied: bool,
    pub(crate) expanded_query: String,
    pub(crate) semantic_fallback: bool,
    pub(crate) semantic_fallback_reason: String,
    pub(crate) search_mode: String,
}

#[derive(Debug, Clone)]
struct SearchResultCandidate {
    result: SearchResultView,
    raw_score: f32,
    final_score: f32,
}

impl Database {
    pub async fn search_documents(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResultView>, sqlx::Error> {
        Ok(self.build_search_results(query, limit).await?.hits)
    }

    pub(crate) fn tantivy_document_count(&self) -> usize {
        self.search_index.doc_count() as usize
    }

    pub(crate) async fn fulltext_repair_needed(&self) -> Result<bool, sqlx::Error> {
        let sqlite_chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
            .fetch_one(&self.pool)
            .await?;
        Ok(sqlite_chunks > 0 && self.tantivy_document_count() == 0)
    }

    pub(crate) async fn repair_empty_fulltext_index<F>(
        &self,
        mut on_progress: F,
    ) -> Result<(), sqlx::Error>
    where
        F: FnMut(usize, usize, String) + Send,
    {
        let sqlite_chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
            .fetch_one(&self.pool)
            .await?;
        if sqlite_chunks == 0 || self.tantivy_document_count() > 0 {
            return Ok(());
        }

        eprintln!("[SeekMind] repairing empty Tantivy index from SQLite chunks={sqlite_chunks}");
        self.search_index
            .clear_all()
            .map_err(sqlx::Error::Protocol)?;

        let documents = sqlx::query_as::<_, FulltextDocumentRow>(
            r#"
            SELECT id, dir_path, path, file_name, ext, file_size, modified_at, content_hash, modified, content
            FROM documents
            ORDER BY rowid
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        let total = documents.len();

        for (index, document) in documents.into_iter().enumerate() {
            let chunks = sqlx::query_as::<_, FulltextChunkRow>(
                r#"
                SELECT heading, snippet, paragraph, page, score
                FROM chunks
                WHERE document_id = ?
                ORDER BY rowid
                "#,
            )
            .bind(&document.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| ChunkRecord {
                heading: row.heading,
                snippet: row.snippet,
                paragraph: row.paragraph,
                page: row.page,
                score: row.score,
                block_indexes: Vec::new(),
            })
            .collect::<Vec<_>>();

            let extracted = ExtractedDocument {
                dir_path: document.dir_path,
                path: document.path,
                file_name: document.file_name,
                ext: document.ext,
                file_size: document.file_size,
                modified_at: document.modified_at,
                content_hash: document.content_hash,
                modified: document.modified,
                content: document.content,
            };

            self.search_index
                .index_document(&document.id, &extracted, &chunks)
                .map_err(sqlx::Error::Protocol)?;
            on_progress(index + 1, total, extracted.file_name);
        }

        eprintln!(
            "[SeekMind] repaired Tantivy index docs={}",
            self.tantivy_document_count()
        );
        Ok(())
    }

    pub async fn record_search_history(
        &self,
        query: &str,
        hit_count: usize,
    ) -> Result<(), sqlx::Error> {
        let normalized_query = normalize_query(query).join(" ");
        if normalized_query.trim().is_empty() {
            return Ok(());
        }

        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT INTO search_history
                (query, normalized_query, hit_count, created_at, last_hit_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(query) DO UPDATE SET
                normalized_query = excluded.normalized_query,
                hit_count = search_history.hit_count + excluded.hit_count,
                last_hit_at = excluded.last_hit_at
            "#,
        )
        .bind(query)
        .bind(normalized_query)
        .bind(hit_count as i64)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_search_history(
        &self,
        limit: i64,
    ) -> Result<Vec<SearchHistoryView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, SearchHistoryRow>(
            r#"
            SELECT query, normalized_query, hit_count, last_hit_at
            FROM search_history
            WHERE trim(query) <> '' AND trim(normalized_query) <> ''
            ORDER BY last_hit_at DESC, hit_count DESC, query ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| SearchHistoryView {
                query: row.query,
                normalized_query: row.normalized_query,
                hit_count: row.hit_count.max(0) as usize,
                last_hit_at: format_unix_ts(row.last_hit_at),
            })
            .collect())
    }

    pub async fn remove_search_history(&self, query: &str) -> Result<(), sqlx::Error> {
        if query.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM search_history WHERE query = ?")
            .bind(query)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn derive_history_terms(
        &self,
        current_terms: &[String],
    ) -> Result<Vec<String>, sqlx::Error> {
        let history = self.list_search_history(30).await?;
        let current_set = current_terms
            .iter()
            .map(|term| term.trim().to_lowercase())
            .filter(|term| !term.is_empty())
            .collect::<HashSet<_>>();

        let mut term_counts = HashMap::<String, usize>::new();
        for item in history {
            let weight = item.hit_count.max(1);
            for term in item.normalized_query.split_whitespace() {
                let normalized = term.trim().to_lowercase();
                if normalized.is_empty()
                    || current_set.contains(&normalized)
                    || normalized.len() < 2
                {
                    continue;
                }
                *term_counts.entry(normalized).or_insert(0) += weight;
            }
        }

        let mut terms = term_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2)
            .collect::<Vec<_>>();
        terms.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        Ok(terms.into_iter().take(4).map(|(term, _)| term).collect())
    }

    pub async fn record_recent_document(
        &self,
        path: &str,
        title: &str,
        file_name: &str,
        ext: &str,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT INTO recent_documents
                (path, title, file_name, ext, last_opened_at, open_count)
            VALUES (?, ?, ?, ?, ?, 1)
            ON CONFLICT(path) DO UPDATE SET
                title = excluded.title,
                file_name = excluded.file_name,
                ext = excluded.ext,
                last_opened_at = excluded.last_opened_at,
                open_count = recent_documents.open_count + 1
            "#,
        )
        .bind(path)
        .bind(title)
        .bind(file_name)
        .bind(ext)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_recent_documents(
        &self,
        limit: i64,
    ) -> Result<Vec<RecentDocumentView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RecentDocumentRow>(
            r#"
            SELECT path, title, file_name, ext, last_opened_at, open_count
            FROM recent_documents
            ORDER BY last_opened_at DESC, open_count DESC, path ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| RecentDocumentView {
                path: row.path,
                title: row.title,
                file_name: row.file_name,
                ext: row.ext,
                last_opened_at: format_unix_ts(row.last_opened_at),
                open_count: row.open_count.max(0) as usize,
            })
            .collect())
    }

    pub async fn remove_recent_document(&self, path: &str) -> Result<(), sqlx::Error> {
        if path.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM recent_documents WHERE path = ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn record_recent_view(
        &self,
        target_type: &str,
        target_id: &str,
        title: &str,
        path: &str,
    ) -> Result<(), sqlx::Error> {
        let target_type = normalize_recent_view_target_type(target_type);
        let target_id = target_id.trim();
        if target_type.is_empty() || target_id.is_empty() {
            return Ok(());
        }

        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT INTO recent_views
                (target_type, target_id, title, path, viewed_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(target_type, target_id) DO UPDATE SET
                title = excluded.title,
                path = excluded.path,
                viewed_at = excluded.viewed_at
            "#,
        )
        .bind(target_type)
        .bind(target_id)
        .bind(title.trim())
        .bind(path.trim())
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_recent_views(&self, limit: i64) -> Result<Vec<RecentViewEntry>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RecentViewRow>(
            r#"
            SELECT target_type, target_id, title, path, viewed_at
            FROM recent_views
            ORDER BY viewed_at DESC, title ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| RecentViewEntry {
                target_type: row.target_type,
                target_id: row.target_id,
                title: row.title,
                path: row.path,
                viewed_at: format_unix_ts(row.viewed_at),
            })
            .collect())
    }

    pub async fn default_embedding_model_available(&self) -> Result<bool, sqlx::Error> {
        #[derive(Debug, sqlx::FromRow)]
        struct DefaultEmbeddingModelRow {
            available: i64,
        }

        let row = sqlx::query_as::<_, DefaultEmbeddingModelRow>(
            r#"
            SELECT available
            FROM embedding_models
            WHERE is_default = 1
            ORDER BY updated_at DESC, name ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|item| item.available != 0).unwrap_or(false))
    }

    pub async fn toggle_result_favorite(
        &self,
        path: &str,
        heading: &str,
        paragraph: Option<u32>,
        page: Option<u32>,
        file_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let target = favorite_result_target(path, heading, paragraph, page);
        let now = current_unix_ts();
        let existing = sqlx::query("SELECT target FROM favorites WHERE target = ?")
            .bind(&target)
            .fetch_optional(&self.pool)
            .await?;

        if existing.is_some() {
            sqlx::query("DELETE FROM favorites WHERE target = ?")
                .bind(&target)
                .execute(&self.pool)
                .await?;
            return Ok(false);
        }

        sqlx::query(
            r#"
            INSERT INTO favorites
                (target, favorite_type, title, path, created_at, updated_at)
            VALUES (?, 'result', ?, ?, ?, ?)
            "#,
        )
        .bind(&target)
        .bind(if heading.trim().is_empty() {
            file_name
        } else {
            heading
        })
        .bind(path)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(true)
    }

    pub(crate) async fn search_documents_debug(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchDebugData, sqlx::Error> {
        self.build_search_results(query, limit).await
    }

    async fn build_search_results(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchDebugData, sqlx::Error> {
        let settings = self.get_index_settings().await?;
        let semantic_enabled = settings.semantic_search_enabled;
        let semantic_weight = settings.semantic_weight.clamp(0.0, 1.0);
        let semantic_threshold = settings.semantic_threshold.clamp(-1.0, 1.0);
        let rewritten_terms = rewrite_query_terms(query);
        let rewritten_query = rewrite_search_text(query);
        let semantic_model_available = self
            .default_embedding_model_available()
            .await
            .unwrap_or(false);
        let history_terms = if settings.prefer_history_enabled {
            self.derive_history_terms(&rewritten_terms)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let history_rewrite_applied = !history_terms.is_empty();
        let mut expanded_terms = rewritten_terms.clone();
        expanded_terms.extend(history_terms.iter().cloned());
        let expanded_query = expanded_terms.join(" ");

        let keyword_hits = self
            .search_index
            .search(&expanded_query, limit.max(1))
            .map_err(sqlx::Error::Protocol)?;
        let recent_documents = if settings.prefer_recent_enabled {
            self.list_recent_documents(50).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let favorites = if settings.prefer_favorites_enabled {
            self.list_favorites(200).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let recent_document_map = recent_documents
            .into_iter()
            .map(|item| (item.path, item.open_count))
            .collect::<HashMap<_, _>>();
        let favorite_targets = favorites
            .into_iter()
            .map(|item| item.target)
            .collect::<HashSet<_>>();

        let semantic_limit = limit.max(1).saturating_mul(3).max(limit.max(1));
        let semantic_result = if semantic_enabled {
            semantic_store::semantic_search_hits(self, query, semantic_limit).await
        } else {
            Ok(Vec::new())
        };
        let (semantic_candidates, semantic_fallback, semantic_fallback_reason) = match semantic_result {
            Ok(hits) => (hits, false, String::new()),
            Err(error) => (Vec::new(), true, error),
        };
        let semantic_fallback_reason_text = if semantic_fallback_reason.is_empty() {
            if !semantic_enabled {
                "语义检索已关闭".to_string()
            } else if !semantic_model_available {
                "语义模型不可用".to_string()
            } else {
                String::new()
            }
        } else {
            semantic_fallback_reason.clone()
        };
        let semantic_candidate_count = semantic_candidates.len();
        let semantic_hits: Vec<_> = semantic_candidates
            .into_iter()
            .filter(|hit| hit.score >= semantic_threshold)
            .collect();
        let semantic_filtered_count = semantic_candidate_count.saturating_sub(semantic_hits.len());

        if keyword_hits.is_empty() && semantic_hits.is_empty() {
            return Ok(SearchDebugData {
                hits: Vec::new(),
                keyword_hit_count: 0,
                semantic_hit_count: 0,
                semantic_candidate_count,
                semantic_filtered_count,
                semantic_enabled,
                semantic_weight,
                semantic_threshold,
                rewritten_terms,
                rewritten_query,
                history_terms,
                history_rewrite_applied,
                expanded_query,
                semantic_fallback: semantic_fallback || !semantic_model_available,
                semantic_fallback_reason: semantic_fallback_reason_text.clone(),
                search_mode: if semantic_enabled {
                    "hybrid".to_string()
                } else {
                    "fulltext".to_string()
                },
            });
        }

        let mut keyword_score_map = HashMap::<String, f32>::new();
        for hit in &keyword_hits {
            keyword_score_map.insert(hit.chunk_id.clone(), hit.score);
        }

        let mut semantic_score_map = HashMap::<String, f32>::new();
        for hit in &semantic_hits {
            semantic_score_map.insert(hit.chunk_id.clone(), hit.score);
        }

        let mut chunk_ids = keyword_hits
            .iter()
            .map(|hit| hit.chunk_id.clone())
            .collect::<Vec<_>>();
        for hit in &semantic_hits {
            if !keyword_score_map.contains_key(&hit.chunk_id) {
                chunk_ids.push(hit.chunk_id.clone());
            }
        }

        let rows = self.fetch_chunks_by_ids(&chunk_ids).await?;
        let mut preview_blocks_by_chunk_id = self.fetch_preview_blocks_for_search_rows(&rows).await?;
        let mut rows_by_id = HashMap::new();
        for row in rows {
            rows_by_id.insert(row.id.clone(), row);
        }

        let mut results = Vec::new();
        let normalized_terms = normalize_query(query);
        let now = current_unix_ts();
        for chunk_id in chunk_ids {
            if let Some(row) = rows_by_id.remove(&chunk_id) {
                let keyword_score = keyword_score_map.get(&chunk_id).copied().unwrap_or(0.0);
                let semantic_score = semantic_score_map.get(&chunk_id).copied().unwrap_or(0.0);
                let title_score = if row.heading.trim().is_empty() {
                    0.0
                } else if super::contains_all_terms(&row.heading, &normalized_terms) {
                    0.32
                } else if super::contains_any_term(&row.heading, &normalized_terms) {
                    0.18
                } else {
                    0.0
                };
                let filename_score = if row.file_name.trim().is_empty() {
                    0.0
                } else if super::contains_all_terms(&row.file_name, &normalized_terms) {
                    0.45
                } else if super::contains_any_term(&row.file_name, &normalized_terms) {
                    0.25
                } else {
                    0.0
                };
                let is_favorite = favorite_targets.contains(&favorite_result_target(
                    &row.path,
                    &row.heading,
                    row.paragraph.map(|value| value as u32),
                    row.page.map(|value| value as u32),
                ));
                let recent_open_count = recent_document_map.get(&row.path).copied().unwrap_or(0);
                let preference_score =
                    super::rerank_bonus(is_favorite, recent_open_count, history_rewrite_applied);
                let (
                    snippet,
                    highlight_spans,
                    snippet_window_start,
                    snippet_window_end,
                    snippet_source_len,
                ) = super::build_search_snippet(&row.snippet, &normalized_terms, 220);
                let (matched_field, match_origin) = if keyword_score > 0.0 {
                    super::matched_field_and_origin(&row, &normalized_terms)
                } else if semantic_score > 0.0 {
                    ("semantic".to_string(), "语义命中".to_string())
                } else {
                    super::matched_field_and_origin(&row, &normalized_terms)
                };
                let rank_reason = super::search_rank_reason(
                    &row,
                    &normalized_terms,
                    &match_origin,
                    keyword_score,
                    semantic_score,
                    semantic_enabled,
                    is_favorite,
                    recent_open_count,
                    history_rewrite_applied,
                    title_score,
                    filename_score,
                    preference_score,
                    row.modified_at,
                    now,
                );
                let base_score = if keyword_score > 0.0 {
                    keyword_score + semantic_score.max(0.0) * semantic_weight
                } else {
                    semantic_score.max(0.0)
                };
                let chunk_weight = row.score.clamp(0.25, 1.0);
                let raw_score =
                    super::boosted_search_score(base_score * chunk_weight, &match_origin, row.modified_at, now);
                let weighted_title_score = title_score * settings.title_weight;
                let weighted_filename_score = filename_score * settings.filename_weight;
                let weighted_preference_score = preference_score * settings.preference_weight;
                let final_score =
                    raw_score + weighted_preference_score + weighted_title_score + weighted_filename_score;
                let mut rank_reason = rank_reason;
                rank_reason.base_score = base_score;
                rank_reason.raw_score = raw_score;
                rank_reason.title_score = weighted_title_score;
                rank_reason.filename_score = weighted_filename_score;
                rank_reason.preference_score = weighted_preference_score;
                results.push(SearchResultCandidate {
                    result: SearchResultView {
                        id: row.id,
                        file_name: row.file_name,
                        path: row.path,
                        ext: row.ext,
                        heading: row.heading.clone(),
                        title_path: row.heading,
                        snippet,
                        matched_field,
                        match_origin,
                        highlight_spans,
                        snippet_window_start,
                        snippet_window_end,
                        snippet_source_len,
                        paragraph: row.paragraph.map(|value| value as u32),
                        page: row.page.map(|value| value as u32),
                        modified: row.modified,
                        score: final_score,
                        rank_reason,
                        preview_blocks: preview_blocks_by_chunk_id
                            .remove(&chunk_id)
                            .unwrap_or_default(),
                    },
                    raw_score,
                    final_score,
                });
            }
        }

        let mut original_rank_map = HashMap::<String, usize>::new();
        let mut original_order = results.clone();
        original_order.sort_by(|left, right| {
            right
                .raw_score
                .partial_cmp(&left.raw_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (index, candidate) in original_order.iter().enumerate() {
            original_rank_map.insert(candidate.result.id.clone(), index + 1);
        }

        results.sort_by(|left, right| {
            right
                .final_score
                .partial_cmp(&left.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut final_results = Vec::new();
        for (index, candidate) in results.into_iter().enumerate() {
            let mut result = candidate.result;
            let original_rank = original_rank_map
                .get(&result.id)
                .copied()
                .unwrap_or(index + 1);
            let final_rank = index + 1;
            result.rank_reason.original_rank = original_rank;
            result.rank_reason.final_rank = final_rank;
            result.rank_reason.rank_delta = original_rank as isize - final_rank as isize;
            final_results.push(result);
        }
        final_results.truncate(limit.max(1));

        Ok(SearchDebugData {
            hits: final_results,
            keyword_hit_count: keyword_hits.len(),
            semantic_hit_count: semantic_hits.len(),
            semantic_candidate_count,
            semantic_filtered_count,
            semantic_enabled,
            semantic_weight,
            semantic_threshold,
            rewritten_terms,
            rewritten_query,
            history_terms,
            history_rewrite_applied,
            expanded_query,
            semantic_fallback: semantic_fallback || !semantic_model_available,
            semantic_fallback_reason: semantic_fallback_reason_text,
            search_mode: if semantic_enabled {
                "hybrid".to_string()
            } else {
                "fulltext".to_string()
            },
        })
    }

    async fn fetch_chunks_by_ids(
        &self,
        chunk_ids: &[String],
    ) -> Result<Vec<SearchRow>, sqlx::Error> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = std::iter::repeat("?")
            .take(chunk_ids.len())
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!(
            r#"
            SELECT
                c.id,
                c.document_id,
                d.file_name,
                d.path,
                d.ext,
                c.heading,
                c.snippet,
                c.paragraph,
                c.page,
                d.modified,
                d.modified_at,
                c.score,
                c.block_indexes_json
            FROM chunks c
            JOIN documents d ON d.id = c.document_id
            WHERE c.id IN ({})
            "#,
            placeholders
        );

        let mut query_builder = sqlx::query_as::<_, SearchRow>(&sql);
        for chunk_id in chunk_ids {
            query_builder = query_builder.bind(chunk_id);
        }

        query_builder.fetch_all(&self.pool).await
    }

    async fn fetch_preview_blocks_for_search_rows(
        &self,
        rows: &[SearchRow],
    ) -> Result<HashMap<String, Vec<PreviewBlockView>>, sqlx::Error> {
        if rows.is_empty() {
            return Ok(HashMap::new());
        }

        let mut document_ids = rows
            .iter()
            .map(|row| row.document_id.clone())
            .collect::<Vec<_>>();
        document_ids.sort();
        document_ids.dedup();

        let placeholders = std::iter::repeat("?")
            .take(document_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            r#"
            SELECT document_id, block_index, block_type, text, heading, level, page, language, markdown, html, asset_path, alt_text, caption, ocr_text
            FROM document_blocks
            WHERE document_id IN ({})
            ORDER BY document_id, block_index
            "#,
            placeholders
        );

        let mut query_builder = sqlx::query_as::<_, BlockWithDocumentRow>(&sql);
        for document_id in &document_ids {
            query_builder = query_builder.bind(document_id);
        }

        let block_rows = query_builder.fetch_all(&self.pool).await?;
        let mut blocks_by_document = HashMap::<String, HashMap<i64, BlockWithDocumentRow>>::new();
        for block in block_rows {
            blocks_by_document
                .entry(block.document_id.clone())
                .or_default()
                .insert(block.block_index, block);
        }

        let mut result = HashMap::<String, Vec<PreviewBlockView>>::new();
        for row in rows {
            let block_indexes: Vec<usize> =
                serde_json::from_str(&row.block_indexes_json).unwrap_or_default();
            let Some(blocks_by_index) = blocks_by_document.get(&row.document_id) else {
                result.insert(row.id.clone(), Vec::new());
                continue;
            };

            let preview_blocks = block_indexes
                .iter()
                .filter_map(|index| {
                    blocks_by_index.get(&(*index as i64)).map(|block| {
                        Self::build_preview_block(
                            &row.path,
                            block.block_index,
                            &block.block_type,
                            &block.text,
                            &block.heading,
                            block.level,
                            block.page,
                            &block.language,
                            &block.markdown,
                            &block.html,
                            &block.asset_path,
                            &block.alt_text,
                            &block.caption,
                            &block.ocr_text,
                        )
                    })
                })
                .collect::<Vec<_>>();
            result.insert(row.id.clone(), preview_blocks);
        }

        Ok(result)
    }
}

fn normalize_recent_view_target_type(target_type: &str) -> String {
    match target_type.trim().to_lowercase().as_str() {
        "document" => "document".to_string(),
        "chunk" => "chunk".to_string(),
        "search" => "search".to_string(),
        "qa_source" => "qa_source".to_string(),
        other if other.is_empty() => String::new(),
        _ => "chunk".to_string(),
    }
}
