/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 本地 SQLite 收藏、集合与关联项存储实现。
 */
use uuid::Uuid;

use crate::seekmind::models::{CollectionItemView, CollectionView, FavoriteView};
use crate::seekmind::storage::types::{CollectionItemInput, CollectionPatchInput};

use super::rows::{CollectionItemRow, CollectionRow, FavoriteRow};
use super::util::{current_unix_ts, format_unix_ts};
use super::Database;

impl Database {
    pub async fn list_favorites(&self, limit: i64) -> Result<Vec<FavoriteView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, FavoriteRow>(
            r#"
            SELECT favorite_type, target, title, path, created_at, updated_at
            FROM favorites
            ORDER BY updated_at DESC, created_at DESC, title ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| FavoriteView {
                favorite_type: row.favorite_type,
                target: row.target,
                title: row.title,
                path: row.path,
                created_at: format_unix_ts(row.created_at),
                updated_at: format_unix_ts(row.updated_at),
            })
            .collect())
    }

    pub async fn remove_favorite(&self, target: &str) -> Result<(), sqlx::Error> {
        if target.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM favorites WHERE target = ?")
            .bind(target)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_collections(&self) -> Result<Vec<CollectionView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT
                c.id,
                c.name,
                c.description,
                c.color,
                c.sort_order,
                COUNT(i.id) AS item_count,
                c.created_at,
                c.updated_at
            FROM collections c
            LEFT JOIN collection_items i ON i.collection_id = c.id
            GROUP BY c.id, c.name, c.description, c.color, c.sort_order, c.created_at, c.updated_at
            ORDER BY c.sort_order ASC, c.updated_at DESC, c.name ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(collection_row_to_view).collect())
    }

    pub async fn create_collection(
        &self,
        name: &str,
        description: &str,
    ) -> Result<CollectionView, sqlx::Error> {
        let now = current_unix_ts();
        let sort_order = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM collections",
        )
        .fetch_one(&self.pool)
        .await?;
        let id = Uuid::new_v4().to_string();
        let normalized_name = normalize_collection_name(name);
        let normalized_description = normalize_collection_description(description);

        sqlx::query(
            r#"
            INSERT INTO collections
                (id, name, description, color, sort_order, created_at, updated_at)
            VALUES (?, ?, ?, '', ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&normalized_name)
        .bind(&normalized_description)
        .bind(sort_order)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(CollectionView {
            id,
            name: normalized_name,
            description: normalized_description,
            color: String::new(),
            sort_order,
            item_count: 0,
            created_at: format_unix_ts(now),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn update_collection(
        &self,
        collection_id: &str,
        patch: &CollectionPatchInput,
    ) -> Result<CollectionView, sqlx::Error> {
        let existing = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT
                c.id,
                c.name,
                c.description,
                c.color,
                c.sort_order,
                COUNT(i.id) AS item_count,
                c.created_at,
                c.updated_at
            FROM collections c
            LEFT JOIN collection_items i ON i.collection_id = c.id
            WHERE c.id = ?
            GROUP BY c.id, c.name, c.description, c.color, c.sort_order, c.created_at, c.updated_at
            LIMIT 1
            "#,
        )
        .bind(collection_id.trim())
        .fetch_optional(&self.pool)
        .await?;

        let Some(existing) = existing else {
            return Err(sqlx::Error::RowNotFound);
        };

        let next_name = patch
            .name
            .as_deref()
            .map(normalize_collection_name)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(existing.name.clone());
        let next_description = patch
            .description
            .as_deref()
            .map(normalize_collection_description)
            .unwrap_or(existing.description.clone());
        let next_color = patch
            .color
            .as_deref()
            .map(normalize_collection_color)
            .unwrap_or(existing.color.clone());
        let now = current_unix_ts();

        sqlx::query(
            r#"
            UPDATE collections
            SET name = ?, description = ?, color = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&next_name)
        .bind(&next_description)
        .bind(&next_color)
        .bind(now)
        .bind(collection_id.trim())
        .execute(&self.pool)
        .await?;

        Ok(CollectionView {
            id: existing.id,
            name: next_name,
            description: next_description,
            color: next_color,
            sort_order: existing.sort_order,
            item_count: existing.item_count.max(0) as usize,
            created_at: format_unix_ts(existing.created_at),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn delete_collection(&self, collection_id: &str) -> Result<(), sqlx::Error> {
        let collection_id = collection_id.trim();
        if collection_id.is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(collection_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_collection_items(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionItemView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, CollectionItemRow>(
            r#"
            SELECT
                id,
                collection_id,
                item_type,
                document_id,
                chunk_id,
                qa_session_id,
                qa_message_id,
                title,
                path,
                title_path,
                snippet,
                note,
                source_meta_json,
                sort_order,
                created_at,
                updated_at
            FROM collection_items
            WHERE collection_id = ?
            ORDER BY sort_order ASC, updated_at DESC, title ASC
            "#,
        )
        .bind(collection_id.trim())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(collection_item_row_to_view).collect())
    }

    pub async fn add_collection_item(
        &self,
        input: &CollectionItemInput,
    ) -> Result<CollectionItemView, sqlx::Error> {
        let collection_id = input.collection_id.trim();
        if collection_id.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }

        let item_type = normalize_collection_item_type(&input.item_type);
        let chunk_id = input.chunk_id.clone().unwrap_or_default();
        let qa_session_id = input.qa_session_id.clone().unwrap_or_default();
        let qa_message_id = input.qa_message_id.clone().unwrap_or_default();
        let title = input.title.trim().to_string();
        let path = input.path.clone().unwrap_or_default();
        let title_path = input.title_path.clone().unwrap_or_default();
        let snippet = input.snippet.clone().unwrap_or_default();
        let note = input.note.clone().unwrap_or_default();
        let source_meta_json = input
            .source_meta_json
            .clone()
            .unwrap_or_else(|| "{}".to_string());
        let now = current_unix_ts();
        let mut document_id = input.document_id.clone().unwrap_or_default();

        if item_type == "document" && document_id.trim().is_empty() {
            // 修复：文档级收藏必须回写真实 document_id，避免同一文档按不同 chunk 重复入库。
            if path.trim().is_empty() {
                eprintln!("[SeekMind] add_collection_item document mode missing path");
            } else if let Some(resolved_document_id) = self.document_id_by_path(&path).await? {
                eprintln!("[SeekMind] add_collection_item document mode resolved document_id for path={path}");
                document_id = resolved_document_id;
            } else {
                eprintln!("[SeekMind] add_collection_item document mode failed to resolve document_id for path={path}");
            }
        }

        if item_type == "document" && document_id.trim().is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }

        let existing_id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT id
            FROM collection_items
            WHERE collection_id = ?
              AND item_type = ?
              AND document_id = ?
              AND chunk_id = ?
              AND qa_session_id = ?
              AND qa_message_id = ?
              AND path = ?
              AND title_path = ?
            LIMIT 1
            "#,
        )
        .bind(collection_id)
        .bind(&item_type)
        .bind(&document_id)
        .bind(&chunk_id)
        .bind(&qa_session_id)
        .bind(&qa_message_id)
        .bind(&path)
        .bind(&title_path)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(existing_id) = existing_id {
            sqlx::query(
                r#"
                UPDATE collection_items
                SET title = ?, snippet = ?, note = ?, source_meta_json = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(&title)
            .bind(&snippet)
            .bind(&note)
            .bind(&source_meta_json)
            .bind(now)
            .bind(&existing_id)
            .execute(&self.pool)
            .await?;

            let row = sqlx::query_as::<_, CollectionItemRow>(
                r#"
                SELECT id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at
                FROM collection_items
                WHERE id = ?
                "#,
            )
            .bind(&existing_id)
            .fetch_one(&self.pool)
            .await?;
            return Ok(collection_item_row_to_view(row));
        }

        let sort_order = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM collection_items WHERE collection_id = ?",
        )
        .bind(collection_id)
        .fetch_one(&self.pool)
        .await?;
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO collection_items
                (id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(collection_id)
        .bind(&item_type)
        .bind(&document_id)
        .bind(&chunk_id)
        .bind(&qa_session_id)
        .bind(&qa_message_id)
        .bind(&title)
        .bind(&path)
        .bind(&title_path)
        .bind(&snippet)
        .bind(&note)
        .bind(&source_meta_json)
        .bind(sort_order)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, CollectionItemRow>(
            r#"
            SELECT id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at
            FROM collection_items
            WHERE id = ?
            "#,
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?;

        Ok(collection_item_row_to_view(row))
    }

    pub async fn update_collection_item_note(
        &self,
        item_id: &str,
        note: &str,
    ) -> Result<CollectionItemView, sqlx::Error> {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return Err(sqlx::Error::RowNotFound);
        }

        let now = current_unix_ts();
        sqlx::query(
            r#"
            UPDATE collection_items
            SET note = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(note.trim())
        .bind(now)
        .bind(item_id)
        .execute(&self.pool)
        .await?;

        let row = sqlx::query_as::<_, CollectionItemRow>(
            r#"
            SELECT id, collection_id, item_type, document_id, chunk_id, qa_session_id, qa_message_id, title, path, title_path, snippet, note, source_meta_json, sort_order, created_at, updated_at
            FROM collection_items
            WHERE id = ?
            "#,
        )
        .bind(item_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(collection_item_row_to_view(row))
    }

    pub async fn remove_collection_item(&self, item_id: &str) -> Result<(), sqlx::Error> {
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM collection_items WHERE id = ?")
            .bind(item_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn export_collection_markdown(
        &self,
        collection_id: &str,
    ) -> Result<String, sqlx::Error> {
        let collection = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT
                c.id,
                c.name,
                c.description,
                c.color,
                c.sort_order,
                COUNT(i.id) AS item_count,
                c.created_at,
                c.updated_at
            FROM collections c
            LEFT JOIN collection_items i ON i.collection_id = c.id
            WHERE c.id = ?
            GROUP BY c.id, c.name, c.description, c.color, c.sort_order, c.created_at, c.updated_at
            LIMIT 1
            "#,
        )
        .bind(collection_id.trim())
        .fetch_optional(&self.pool)
        .await?;

        let Some(collection) = collection else {
            return Err(sqlx::Error::RowNotFound);
        };

        let items = self.list_collection_items(collection_id).await?;
        let mut markdown = String::new();
        markdown.push_str(&format!("# {}\n\n", collection.name));
        if !collection.description.trim().is_empty() {
            markdown.push_str(&format!("> {}\n\n", collection.description.trim()));
        }

        for (index, item) in items.iter().enumerate() {
            markdown.push_str(&format!("## {}. {}\n\n", index + 1, item.title.trim()));
            markdown.push_str(&format!("- 类型：{}\n", item.item_type));
            if !item.path.trim().is_empty() {
                markdown.push_str(&format!("- 路径：{}\n", item.path.trim()));
            }
            if !item.title_path.trim().is_empty() {
                markdown.push_str(&format!("- 定位：{}\n", item.title_path.trim()));
            }
            if !item.snippet.trim().is_empty() {
                markdown.push_str("\n摘录：\n\n");
                markdown.push_str(&format!("> {}\n", item.snippet.trim()));
            }
            if !item.note.trim().is_empty() {
                markdown.push_str("\n备注：\n\n");
                markdown.push_str(&item.note.trim());
                markdown.push('\n');
            }
            markdown.push('\n');
        }

        Ok(markdown)
    }
}

fn normalize_collection_name(name: &str) -> String {
    let normalized = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "新主题集合".to_string();
    }

    normalized.chars().take(40).collect()
}

fn normalize_collection_description(description: &str) -> String {
    description.trim().chars().take(200).collect()
}

fn normalize_collection_color(color: &str) -> String {
    color.trim().chars().take(32).collect()
}

fn normalize_collection_item_type(item_type: &str) -> String {
    match item_type.trim().to_lowercase().as_str() {
        "document" => "document".to_string(),
        "chunk" => "chunk".to_string(),
        "search" => "search".to_string(),
        "qa_source" => "qa_source".to_string(),
        other if other.is_empty() => "chunk".to_string(),
        _ => "chunk".to_string(),
    }
}

pub(crate) fn favorite_result_target(
    path: &str,
    heading: &str,
    paragraph: Option<u32>,
    page: Option<u32>,
) -> String {
    format!(
        "result|{}|{}|{}|{}",
        path,
        heading.trim(),
        paragraph.map(|value| value.to_string()).unwrap_or_default(),
        page.map(|value| value.to_string()).unwrap_or_default()
    )
}

fn collection_row_to_view(row: CollectionRow) -> CollectionView {
    CollectionView {
        id: row.id,
        name: row.name,
        description: row.description,
        color: row.color,
        sort_order: row.sort_order,
        item_count: row.item_count.max(0) as usize,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}

fn collection_item_row_to_view(row: CollectionItemRow) -> CollectionItemView {
    CollectionItemView {
        id: row.id,
        collection_id: row.collection_id,
        item_type: row.item_type,
        document_id: row.document_id,
        chunk_id: row.chunk_id,
        qa_session_id: row.qa_session_id,
        qa_message_id: row.qa_message_id,
        title: row.title,
        path: row.path,
        title_path: row.title_path,
        snippet: row.snippet,
        note: row.note,
        source_meta_json: row.source_meta_json,
        sort_order: row.sort_order,
        created_at: format_unix_ts(row.created_at),
        updated_at: format_unix_ts(row.updated_at),
    }
}
