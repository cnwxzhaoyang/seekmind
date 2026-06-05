/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description DocMind 本地 SQLite 问答会话与历史存储实现。
 */
use uuid::Uuid;

use crate::docmind::models::{QaAnswerView, QaHistoryView, QaMessageView, QaSessionView};

use super::rows::{QaHistoryRow, QaMessageRow, QaSessionRow};
use super::util::{current_unix_ts, format_unix_ts};
use super::Database;

impl Database {
    pub async fn list_qa_history(&self, limit: i64) -> Result<Vec<QaHistoryView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, QaHistoryRow>(
            r#"
            SELECT id, question, answer, state, sources_json, retrieval_json, model, error, warning, created_at
            FROM qa_history
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(super::qa_history_row_to_view)
            .collect())
    }

    pub async fn remove_qa_history(&self, id: &str) -> Result<(), sqlx::Error> {
        if id.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM qa_history WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn record_qa_history(&self, answer: &QaAnswerView) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        let sources_json =
            serde_json::to_string(&answer.sources).unwrap_or_else(|_| "[]".to_string());
        let retrieval_json =
            serde_json::to_string(&answer.retrieval).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_history
                (id, question, answer, state, sources_json, retrieval_json, model, error, warning, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&answer.id)
        .bind(&answer.question)
        .bind(&answer.answer)
        .bind(&answer.state)
        .bind(sources_json)
        .bind(retrieval_json)
        .bind(&answer.model)
        .bind(answer.error.as_deref().unwrap_or(""))
        .bind(answer.warning.as_deref().unwrap_or(""))
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_qa_session(&self, title: &str) -> Result<QaSessionView, sqlx::Error> {
        let now = current_unix_ts();
        let id = Uuid::new_v4().to_string();
        let normalized_title = super::normalize_qa_session_title(title);
        sqlx::query(
            r#"
            INSERT INTO qa_sessions (id, title, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&normalized_title)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(QaSessionView {
            id,
            title: normalized_title,
            message_count: 0,
            created_at: format_unix_ts(now),
            updated_at: format_unix_ts(now),
        })
    }

    pub async fn list_qa_sessions(&self, limit: i64) -> Result<Vec<QaSessionView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, QaSessionRow>(
            r#"
            SELECT
                s.id,
                s.title,
                COUNT(m.id) AS message_count,
                s.created_at,
                s.updated_at
            FROM qa_sessions s
            LEFT JOIN qa_messages m ON m.session_id = s.id
            GROUP BY s.id, s.title, s.created_at, s.updated_at
            ORDER BY s.updated_at DESC, s.created_at DESC, s.title ASC
            LIMIT ?
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(super::qa_session_row_to_view).collect())
    }

    pub async fn list_qa_messages(
        &self,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<QaMessageView>, sqlx::Error> {
        if session_id.trim().is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, QaMessageRow>(
            r#"
            SELECT id, session_id, question, answer, state, sources_json, retrieval_json, model, error, warning, created_at, updated_at
            FROM qa_messages
            WHERE session_id = ?
            ORDER BY created_at ASC
            LIMIT ?
            "#,
        )
        .bind(session_id)
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(super::qa_message_row_to_view).collect())
    }

    pub async fn list_qa_messages_recent(
        &self,
        session_id: &str,
        limit: i64,
    ) -> Result<Vec<QaMessageView>, sqlx::Error> {
        if session_id.trim().is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, QaMessageRow>(
            r#"
            SELECT id, session_id, question, answer, state, sources_json, retrieval_json, model, error, warning, created_at, updated_at
            FROM qa_messages
            WHERE session_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(session_id)
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(super::qa_message_row_to_view).collect())
    }

    pub async fn remove_qa_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        if session_id.trim().is_empty() {
            return Ok(());
        }

        sqlx::query("DELETE FROM qa_sessions WHERE id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_qa_session_title(
        &self,
        session_id: &str,
        title: &str,
    ) -> Result<(), sqlx::Error> {
        let normalized_title = super::normalize_qa_session_title(title);
        let now = current_unix_ts();
        sqlx::query(
            r#"
            UPDATE qa_sessions
            SET title = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&normalized_title)
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_qa_answer(
        &self,
        answer: &QaAnswerView,
        session_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.record_qa_history(answer).await?;
        if let Some(session_id) = session_id {
            self.record_qa_message(session_id, answer).await?;
        }
        Ok(())
    }

    pub async fn record_qa_message(
        &self,
        session_id: &str,
        answer: &QaAnswerView,
    ) -> Result<(), sqlx::Error> {
        if session_id.trim().is_empty() {
            return Ok(());
        }

        let now = current_unix_ts();
        let sources_json =
            serde_json::to_string(&answer.sources).unwrap_or_else(|_| "[]".to_string());
        let retrieval_json =
            serde_json::to_string(&answer.retrieval).unwrap_or_else(|_| "{}".to_string());
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO qa_messages
                (id, session_id, question, answer, state, sources_json, retrieval_json, model, error, warning, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&answer.id)
        .bind(session_id)
        .bind(&answer.question)
        .bind(&answer.answer)
        .bind(&answer.state)
        .bind(sources_json)
        .bind(retrieval_json)
        .bind(&answer.model)
        .bind(answer.error.as_deref().unwrap_or(""))
        .bind(answer.warning.as_deref().unwrap_or(""))
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            UPDATE qa_sessions
            SET updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(now)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
