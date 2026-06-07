/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 本地 SQLite 标签相关存储实现。
 */
use uuid::Uuid;

use crate::seekmind::models::TagView;
use crate::seekmind::storage::types::TagPatchInput;

use super::rows::{TagRow, TargetTagRow};
use super::util::{current_unix_ts, format_unix_ts};
use super::Database;

impl Database {
    pub async fn list_tags(&self) -> Result<Vec<TagView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                COUNT(it.id) AS target_count,
                t.created_at,
                t.updated_at
            FROM tags t
            LEFT JOIN item_tags it ON it.tag_id = t.id
            GROUP BY t.id, t.name, t.color, t.created_at, t.updated_at
            ORDER BY t.updated_at DESC, t.created_at DESC, t.name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(tag_row_to_view).collect())
    }

    pub async fn create_tag(&self, name: &str, color: &str) -> Result<TagView, sqlx::Error> {
        let normalized_name = normalize_tag_name(name);
        let normalized_color = normalize_tag_color(color);
        let now = current_unix_ts();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO tags (id, name, color, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&normalized_name)
        .bind(&normalized_color)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(TagView {
            id,
            name: normalized_name,
            color: normalized_color,
            target_count: 0,
            created_at: format_unix_ts(now),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn update_tag(
        &self,
        tag_id: &str,
        patch: &TagPatchInput,
    ) -> Result<TagView, sqlx::Error> {
        let existing = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                COUNT(it.id) AS target_count,
                t.created_at,
                t.updated_at
            FROM tags t
            LEFT JOIN item_tags it ON it.tag_id = t.id
            WHERE t.id = ?
            GROUP BY t.id, t.name, t.color, t.created_at, t.updated_at
            "#,
        )
        .bind(tag_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(existing) = existing else {
            return Err(sqlx::Error::RowNotFound);
        };

        let name = patch
            .name
            .as_deref()
            .map(normalize_tag_name)
            .unwrap_or(existing.name);
        let color = patch
            .color
            .as_deref()
            .map(normalize_tag_color)
            .unwrap_or(existing.color);
        let now = current_unix_ts();
        sqlx::query(
            r#"
            UPDATE tags
            SET name = ?, color = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&name)
        .bind(&color)
        .bind(now)
        .bind(tag_id)
        .execute(&self.pool)
        .await?;

        Ok(TagView {
            id: existing.id,
            name,
            color,
            target_count: existing.target_count.max(0) as usize,
            created_at: format_unix_ts(existing.created_at),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn delete_tag(&self, tag_id: &str) -> Result<(), sqlx::Error> {
        if tag_id.trim().is_empty() {
            return Ok(());
        }
        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(tag_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_target_tags(
        &self,
        target_type: &str,
        target_id: &str,
    ) -> Result<Vec<TagView>, sqlx::Error> {
        let target_type = normalize_tag_target_type(target_type);
        if target_type.is_empty() || target_id.trim().is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, TargetTagRow>(
            r#"
            SELECT t.id, t.name, t.color, t.created_at, t.updated_at
            FROM item_tags it
            JOIN tags t ON t.id = it.tag_id
            WHERE it.target_type = ? AND it.target_id = ?
            ORDER BY t.updated_at DESC, t.name ASC
            "#,
        )
        .bind(target_type)
        .bind(target_id.trim())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| TagView {
                id: row.id,
                name: row.name,
                color: row.color,
                target_count: 0,
                created_at: format_unix_ts(row.created_at),
                updated_at: format_unix_ts(row.updated_at),
            })
            .collect())
    }

    pub async fn add_tag_to_target(
        &self,
        target_type: &str,
        target_id: &str,
        name: &str,
        color: &str,
    ) -> Result<TagView, sqlx::Error> {
        let target_type = normalize_tag_target_type(target_type);
        let target_id = target_id.trim();
        if target_type.is_empty() || target_id.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }

        let normalized_name = normalize_tag_name(name);
        if normalized_name.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }
        let normalized_color = normalize_tag_color(color);
        let existing = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                COUNT(it.id) AS target_count,
                t.created_at,
                t.updated_at
            FROM tags t
            LEFT JOIN item_tags it ON it.tag_id = t.id
            WHERE lower(t.name) = lower(?)
            GROUP BY t.id, t.name, t.color, t.created_at, t.updated_at
            "#,
        )
        .bind(&normalized_name)
        .fetch_optional(&self.pool)
        .await?;

        let tag = if let Some(existing) = existing {
            existing
        } else {
            let now = current_unix_ts();
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO tags (id, name, color, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&normalized_name)
            .bind(&normalized_color)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
            TagRow {
                id,
                name: normalized_name.clone(),
                color: normalized_color.clone(),
                target_count: 0,
                created_at: now,
                updated_at: now,
            }
        };

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO item_tags (tag_id, target_type, target_id)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(&tag.id)
        .bind(&target_type)
        .bind(target_id)
        .execute(&self.pool)
        .await?;

        let now = current_unix_ts();
        sqlx::query(
            r#"
            UPDATE tags
            SET updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(now)
        .bind(&tag.id)
        .execute(&self.pool)
        .await?;

        let updated = sqlx::query_as::<_, TagRow>(
            r#"
            SELECT
                t.id,
                t.name,
                t.color,
                COUNT(it.id) AS target_count,
                t.created_at,
                t.updated_at
            FROM tags t
            LEFT JOIN item_tags it ON it.tag_id = t.id
            WHERE t.id = ?
            GROUP BY t.id, t.name, t.color, t.created_at, t.updated_at
            "#,
        )
        .bind(&tag.id)
        .fetch_one(&self.pool)
        .await?;

        Ok(tag_row_to_view(updated))
    }

    pub async fn remove_tag_from_target(
        &self,
        target_type: &str,
        target_id: &str,
        tag_id: &str,
    ) -> Result<(), sqlx::Error> {
        let target_type = normalize_tag_target_type(target_type);
        if target_type.is_empty() || target_id.trim().is_empty() || tag_id.trim().is_empty() {
            return Ok(());
        }

        sqlx::query(
            r#"
            DELETE FROM item_tags
            WHERE target_type = ? AND target_id = ? AND tag_id = ?
            "#,
        )
        .bind(target_type)
        .bind(target_id.trim())
        .bind(tag_id.trim())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn normalize_tag_name(name: &str) -> String {
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "未命名标签".to_string();
    }

    normalized.chars().take(40).collect()
}

fn normalize_tag_color(color: &str) -> String {
    let trimmed = color.trim();
    if trimmed.is_empty() {
        return "#3B82F6".to_string();
    }
    trimmed.chars().take(32).collect()
}

fn normalize_tag_target_type(target_type: &str) -> String {
    match target_type.trim().to_lowercase().as_str() {
        "document" => "document".to_string(),
        "chunk" => "chunk".to_string(),
        "search" => "search".to_string(),
        "qa_source" => "qa_source".to_string(),
        other if other.is_empty() => String::new(),
        _ => "chunk".to_string(),
    }
}

fn tag_row_to_view(row: TagRow) -> TagView {
    TagView {
        id: row.id,
        name: row.name,
        color: row.color,
        target_count: row.target_count.max(0) as usize,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}
