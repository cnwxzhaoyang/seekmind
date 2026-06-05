/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description DocMind 数据库设置与模型连接相关逻辑。
 */

use uuid::Uuid;

use crate::docmind::models::{QaModelProfileUpsertView, QaModelProfileView};
use crate::docmind::storage::types::{IndexSettings, NetworkProxySettings, QaSettings};

use super::rows::{NetworkProxySettingsRow, QaModelProfileRow, QaSettingsRow};
use super::util::{current_unix_ts, default_exclude_dirs, format_unix_ts};
use super::{default_network_proxy_settings, default_qa_settings, qa_model_profile_row_to_view, Database};

impl Database {
    pub async fn get_index_settings(&self) -> Result<IndexSettings, sqlx::Error> {
        #[derive(Debug, sqlx::FromRow)]
        struct IndexSettingsRow {
            exclude_dirs: String,
            exclude_exts: String,
            max_file_size_mb: i64,
            semantic_search_enabled: i64,
            semantic_weight: f32,
            semantic_threshold: f32,
            title_weight: f32,
            filename_weight: f32,
            preference_weight: f32,
            prefer_favorites_enabled: i64,
            prefer_recent_enabled: i64,
            prefer_history_enabled: i64,
        }

        let row = sqlx::query_as::<_, IndexSettingsRow>(
            r#"
            SELECT exclude_dirs, exclude_exts, max_file_size_mb, semantic_search_enabled, semantic_weight, semantic_threshold, title_weight, filename_weight, preference_weight, prefer_favorites_enabled, prefer_recent_enabled, prefer_history_enabled
            FROM index_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(IndexSettings {
                exclude_dirs: serde_json::from_str(&row.exclude_dirs).unwrap_or_default(),
                exclude_exts: serde_json::from_str(&row.exclude_exts).unwrap_or_default(),
                max_file_size_mb: row.max_file_size_mb.max(0) as u64,
                semantic_search_enabled: row.semantic_search_enabled != 0,
                semantic_weight: row.semantic_weight.clamp(0.0, 1.0),
                semantic_threshold: row.semantic_threshold.clamp(-1.0, 1.0),
                title_weight: row.title_weight.clamp(0.0, 3.0),
                filename_weight: row.filename_weight.clamp(0.0, 3.0),
                preference_weight: row.preference_weight.clamp(0.0, 3.0),
                prefer_favorites_enabled: row.prefer_favorites_enabled != 0,
                prefer_recent_enabled: row.prefer_recent_enabled != 0,
                prefer_history_enabled: row.prefer_history_enabled != 0,
            })
        } else {
            Ok(IndexSettings {
                exclude_dirs: default_exclude_dirs(),
                exclude_exts: Vec::new(),
                max_file_size_mb: 50,
                semantic_search_enabled: true,
                semantic_weight: 0.25,
                semantic_threshold: 0.2,
                title_weight: 1.0,
                filename_weight: 1.0,
                preference_weight: 1.0,
                prefer_favorites_enabled: true,
                prefer_recent_enabled: true,
                prefer_history_enabled: true,
            })
        }
    }

    pub async fn save_index_settings(&self, settings: &IndexSettings) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_settings
                (id, exclude_dirs, exclude_exts, max_file_size_mb, semantic_search_enabled, semantic_weight, semantic_threshold, title_weight, filename_weight, preference_weight, prefer_favorites_enabled, prefer_recent_enabled, prefer_history_enabled)
            VALUES (
                1,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?
            )
            "#,
        )
        .bind(serde_json::to_string(&settings.exclude_dirs).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&settings.exclude_exts).unwrap_or_else(|_| "[]".to_string()))
        .bind(settings.max_file_size_mb as i64)
        .bind(settings.semantic_search_enabled as i64)
        .bind(settings.semantic_weight)
        .bind(settings.semantic_threshold)
        .bind(settings.title_weight)
        .bind(settings.filename_weight)
        .bind(settings.preference_weight)
        .bind(settings.prefer_favorites_enabled as i64)
        .bind(settings.prefer_recent_enabled as i64)
        .bind(settings.prefer_history_enabled as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_qa_settings(&self) -> Result<QaSettings, sqlx::Error> {
        let row = sqlx::query_as::<_, QaSettingsRow>(
            r#"
            SELECT enabled, provider, base_url, api_key, model, temperature, max_output_tokens, context_chunk_limit, context_token_budget, min_evidence_count, min_retrieval_score, updated_at
            FROM qa_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(QaSettings {
                enabled: row.enabled != 0,
                provider: row.provider,
                base_url: row.base_url,
                api_key: row.api_key,
                model: row.model,
                temperature: row.temperature.clamp(0.0, 2.0),
                max_output_tokens: row.max_output_tokens.max(1) as usize,
                context_chunk_limit: row.context_chunk_limit.max(1) as usize,
                context_token_budget: row.context_token_budget.max(1) as usize,
                min_evidence_count: row.min_evidence_count.max(1) as usize,
                min_retrieval_score: row.min_retrieval_score,
            })
        } else {
            Ok(default_qa_settings())
        }
    }

    pub async fn get_qa_settings_updated_at(&self) -> Result<String, sqlx::Error> {
        let updated_at = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT updated_at
            FROM qa_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);

        Ok(format_unix_ts(updated_at))
    }

    pub async fn get_network_proxy_settings(&self) -> Result<NetworkProxySettings, sqlx::Error> {
        let row = sqlx::query_as::<_, NetworkProxySettingsRow>(
            r#"
            SELECT enabled, proxy_url, updated_at
            FROM network_proxy_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(NetworkProxySettings {
                enabled: row.enabled != 0,
                proxy_url: row.proxy_url,
            })
        } else {
            Ok(default_network_proxy_settings())
        }
    }

    pub async fn get_network_proxy_settings_updated_at(&self) -> Result<String, sqlx::Error> {
        let updated_at = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT updated_at
            FROM network_proxy_settings
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);

        Ok(format_unix_ts(updated_at))
    }

    pub async fn save_qa_settings(&self, settings: &QaSettings) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_settings
                (id, enabled, provider, base_url, api_key, model, temperature, max_output_tokens, context_chunk_limit, context_token_budget, min_evidence_count, min_retrieval_score, updated_at)
            VALUES (
                1,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?,
                ?
            )
            "#,
        )
        .bind(settings.enabled as i64)
        .bind(settings.provider.trim())
        .bind(settings.base_url.trim())
        .bind(settings.api_key.as_str())
        .bind(settings.model.trim())
        .bind(settings.temperature)
        .bind(settings.max_output_tokens as i64)
        .bind(settings.context_chunk_limit as i64)
        .bind(settings.context_token_budget as i64)
        .bind(settings.min_evidence_count as i64)
        .bind(settings.min_retrieval_score)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_qa_model_profiles(&self) -> Result<Vec<QaModelProfileView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            ORDER BY is_default DESC, updated_at DESC, created_at DESC, name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(qa_model_profile_row_to_view).collect())
    }

    pub async fn get_qa_model_profile(
        &self,
        profile_id: &str,
    ) -> Result<Option<QaModelProfileView>, sqlx::Error> {
        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(qa_model_profile_row_to_view))
    }

    pub async fn save_qa_model_profile(
        &self,
        profile: &QaModelProfileUpsertView,
    ) -> Result<QaModelProfileView, sqlx::Error> {
        let now = current_unix_ts();
        let id = profile
            .id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let existing = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_optional(&self.pool)
        .await?;
        let created_at = existing.as_ref().map(|item| item.created_at).unwrap_or(now);

        if profile.is_default {
            sqlx::query("UPDATE qa_model_profiles SET is_default = 0, updated_at = ?")
                .bind(now)
                .execute(&self.pool)
                .await?;
        }

        // 修复：模型连接保存后即视为可用，统一写入 enabled=1，避免默认连接与可用状态分裂。
        let enabled = true;
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_model_profiles
                (id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(profile.name.trim())
        .bind(profile.provider.trim())
        .bind(profile.base_url.trim())
        .bind(profile.api_key.as_str())
        .bind(profile.model.trim())
        .bind(enabled as i64)
        .bind(profile.is_default as i64)
        .bind(created_at)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;

        Ok(qa_model_profile_row_to_view(row))
    }

    pub async fn remove_qa_model_profile(&self, profile_id: &str) -> Result<(), sqlx::Error> {
        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        let was_default = row.map(|item| item.is_default != 0).unwrap_or(false);
        sqlx::query("DELETE FROM qa_model_profiles WHERE id = ?")
            .bind(profile_id)
            .execute(&self.pool)
            .await?;

        if was_default {
            let next_default = sqlx::query_scalar::<_, String>(
                r#"
                SELECT id
                FROM qa_model_profiles
                ORDER BY updated_at DESC, created_at DESC, name ASC
                LIMIT 1
                "#,
            )
            .fetch_optional(&self.pool)
            .await?;

            if let Some(next_default) = next_default {
                self.set_default_qa_model_profile(&next_default).await?;
            }
        }

        Ok(())
    }

    pub async fn set_default_qa_model_profile(
        &self,
        profile_id: &str,
    ) -> Result<QaModelProfileView, sqlx::Error> {
        let now = current_unix_ts();
        let row = sqlx::query_as::<_, QaModelProfileRow>(
            r#"
            SELECT id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at
            FROM qa_model_profiles
            WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Err(sqlx::Error::RowNotFound);
        };

        sqlx::query("UPDATE qa_model_profiles SET is_default = 0, updated_at = ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        // 修复：默认连接即当前启用连接，设默认时同步写入 enabled=1，避免列表默认项和实际可用项不一致。
        sqlx::query(
            "UPDATE qa_model_profiles SET is_default = 1, enabled = 1, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(profile_id)
        .execute(&self.pool)
        .await?;

        Ok(qa_model_profile_row_to_view(QaModelProfileRow {
            is_default: 1,
            enabled: 1,
            updated_at: now,
            ..row
        }))
    }

    pub async fn save_network_proxy_settings(
        &self,
        settings: &NetworkProxySettings,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO network_proxy_settings
                (id, enabled, proxy_url, updated_at)
            VALUES (1, ?, ?, ?)
            "#,
        )
        .bind(settings.enabled as i64)
        .bind(settings.proxy_url.trim())
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
