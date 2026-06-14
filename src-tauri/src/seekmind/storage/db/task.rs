/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 索引任务、checkpoint 与运行态数据库逻辑。
 */

use sqlx::Row;

use crate::seekmind::models::{CurrentTaskView, FailedFileView};

use super::rows::{CurrentTaskRow, FailedFileRow, IndexCheckpointRow, IndexRunSummaryRow};
use super::util::{
    current_unix_ts, format_unix_ts, is_virtual_directory, normalize_path_for_comparison,
    normalized_like_prefix, scalar_count_bind,
};
use super::Database;

impl Database {
    pub(crate) async fn recover_interrupted_index_task(&self) -> Result<(), sqlx::Error> {
        let checkpoint_exists = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM index_checkpoint
            WHERE id = 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?
            > 0;

        let current_task = sqlx::query_as::<_, CurrentTaskRow>(
            r#"
            SELECT label, details, state, current_dir, current_file, started_at, progress, scanned, total, succeeded, failed, updated, skipped, deleted, warning, pause_requested
            FROM current_task
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        let Some(task) = current_task else {
            return Ok(());
        };

        if task.state != "running" {
            return Ok(());
        }

        if checkpoint_exists {
            // 修复：桌面端热重载或异常退出后，内存中的索引线程已经不存在，但数据库还残留 running。
            // 启动时把它恢复成可继续的 paused，避免状态页一直假装卡在某个文件上。
            self.set_current_task(
                &task.label,
                "检测到上次索引任务中断，已恢复为暂停，可点击继续",
                "paused",
                &task.current_dir,
                &task.current_file,
                task.progress.clamp(0, 100) as u8,
                task.scanned.max(0) as usize,
                task.total.max(0) as usize,
                task.succeeded.max(0) as usize,
                task.failed.max(0) as usize,
                task.updated.max(0) as usize,
                task.skipped.max(0) as usize,
                task.deleted.max(0) as usize,
                Some("上次任务被中断"),
                true,
            )
            .await?;
            eprintln!(
                "[SeekMind] recovered interrupted index task as paused file={}",
                task.current_file
            );
        } else {
            self.clear_current_task().await?;
            eprintln!("[SeekMind] cleared stale running index task without checkpoint");
        }

        Ok(())
    }

    pub(crate) async fn save_index_run_summary(
        &self,
        updated: usize,
        skipped: usize,
        deleted: usize,
        scanned: usize,
        total: usize,
        succeeded: usize,
        failed: usize,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_run_summary
                (id, updated, skipped, deleted, scanned, total, succeeded, failed, completed_at)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(updated as i64)
        .bind(skipped as i64)
        .bind(deleted as i64)
        .bind(scanned as i64)
        .bind(total as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn last_index_run_summary(
        &self,
    ) -> Result<Option<crate::seekmind::models::IndexRunSummaryView>, sqlx::Error> {
        let row = sqlx::query_as::<_, IndexRunSummaryRow>(
            r#"
            SELECT updated, skipped, deleted, scanned, total, succeeded, failed, completed_at
            FROM index_run_summary
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| crate::seekmind::models::IndexRunSummaryView {
            updated: row.updated as usize,
            skipped: row.skipped as usize,
            deleted: row.deleted as usize,
            scanned: row.scanned as usize,
            total: row.total as usize,
            succeeded: row.succeeded as usize,
            failed: row.failed as usize,
            completed_at: format_unix_ts(row.completed_at),
        }))
    }

    pub(crate) async fn save_index_checkpoint(
        &self,
        dir_paths: &[String],
        pending_delete_paths: &[String],
        pending_update_paths: &[String],
        phase: &str,
        current_dir: &str,
        current_file: &str,
        total: usize,
        processed: usize,
        succeeded: usize,
        failed: usize,
        updated: usize,
        skipped: usize,
        deleted: usize,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO index_checkpoint
                (id, dir_paths, pending_delete_paths, pending_update_paths, phase, current_dir, current_file, total, processed, succeeded, failed, updated, skipped, deleted)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(serde_json::to_string(dir_paths).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(pending_delete_paths).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(pending_update_paths).unwrap_or_else(|_| "[]".to_string()))
        .bind(phase)
        .bind(current_dir)
        .bind(current_file)
        .bind(total as i64)
        .bind(processed as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
        .bind(updated as i64)
        .bind(skipped as i64)
        .bind(deleted as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn load_index_checkpoint(
        &self,
    ) -> Result<Option<IndexCheckpointRow>, sqlx::Error> {
        sqlx::query_as::<_, IndexCheckpointRow>(
            r#"
            SELECT dir_paths, pending_delete_paths, pending_update_paths, phase, current_dir, current_file, total, processed, succeeded, failed, updated, skipped, deleted
            FROM index_checkpoint
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub(crate) async fn clear_index_checkpoint(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM index_checkpoint WHERE id = 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn set_current_task(
        &self,
        label: &str,
        details: &str,
        state: &str,
        current_dir: &str,
        current_file: &str,
        progress: u8,
        scanned: usize,
        total: usize,
        succeeded: usize,
        failed: usize,
        updated: usize,
        skipped: usize,
        deleted: usize,
        warning: Option<&str>,
        pause_requested: bool,
    ) -> Result<(), sqlx::Error> {
        let now = current_unix_ts();
        let existing_started_at = sqlx::query_as::<_, CurrentTaskRow>(
            r#"
            SELECT label, details, state, current_dir, current_file, started_at, progress, scanned, total, succeeded, failed, updated, skipped, deleted, warning, pause_requested
            FROM current_task
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .map(|row| {
            // 修复：运行中/暂停中的任务更新时间要沿用首次启动时间，避免耗时跳变。
            if matches!(row.state.as_str(), "running" | "paused") {
                row.started_at.max(0)
            } else {
                0
            }
        })
        .unwrap_or(0);
        let started_at = if existing_started_at > 0 {
            existing_started_at
        } else {
            now
        };
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO current_task
                (id, label, details, state, current_dir, current_file, started_at, progress, scanned, total, succeeded, failed, updated, skipped, deleted, warning, pause_requested)
            VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(label)
        .bind(details)
        .bind(state)
        .bind(current_dir)
        .bind(current_file)
        .bind(started_at)
        .bind(progress as i64)
        .bind(scanned as i64)
        .bind(total as i64)
        .bind(succeeded as i64)
        .bind(failed as i64)
        .bind(updated as i64)
        .bind(skipped as i64)
        .bind(deleted as i64)
        .bind(warning)
        .bind(pause_requested as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn clear_current_task(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM current_task WHERE id = 1")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn request_pause_current_task(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE current_task
            SET pause_requested = 1
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn clear_pause_request(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE current_task
            SET pause_requested = 0
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn current_task_pause_requested(&self) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT pause_requested FROM current_task WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;

        Ok(row
            .and_then(|row| row.try_get::<i64, _>(0).ok())
            .map(|value| value != 0)
            .unwrap_or(false))
    }

    pub(crate) async fn clear_all_index_data(&self) -> Result<(), sqlx::Error> {
        self.search_index
            .clear_all()
            .map_err(sqlx::Error::Protocol)?;

        sqlx::query("DELETE FROM index_checkpoint")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM current_task")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM index_run_summary")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM failed_files")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM chunk_embeddings")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"
            UPDATE vector_index_meta
            SET chunk_count = 0,
                last_indexed_at = 0,
                last_error = '',
                status = 'idle'
            WHERE id = 1
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("DELETE FROM document_blocks")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM chunks")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM documents")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"
            UPDATE index_dirs
            SET docs = 0,
                chunks = 0,
                status = 'pending'
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn set_index_dir_status(
        &self,
        path: &str,
        docs: usize,
        chunks: usize,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE index_dirs
            SET docs = ?, chunks = ?, status = ?
            WHERE path = ?
            "#,
        )
        .bind(docs as i64)
        .bind(chunks as i64)
        .bind(status)
        .bind(path)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn refresh_index_dir_stats(&self, path: &str) -> Result<(), sqlx::Error> {
        let (docs, chunks) = if is_virtual_directory(path) {
            let docs = scalar_count_bind(
                &self.pool,
                r#"
                SELECT COUNT(*)
                FROM documents
                WHERE dir_path = ?
                "#,
                path,
            )
            .await? as usize;

            let chunks = scalar_count_bind(
                &self.pool,
                r#"
                SELECT COUNT(*)
                FROM chunks c
                INNER JOIN documents d ON d.id = c.document_id
                WHERE d.dir_path = ?
                "#,
                path,
            )
            .await? as usize;
            (docs, chunks)
        } else {
            let normalized_path = normalize_path_for_comparison(path);
            let prefix = normalized_like_prefix(path);
            let docs = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM documents
                WHERE REPLACE(path, CHAR(92), '/') = ?
                   OR REPLACE(path, CHAR(92), '/') LIKE ?
                "#,
            )
            .bind(normalized_path.clone())
            .bind(&prefix)
            .fetch_one(&self.pool)
            .await? as usize;

            let chunks = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COUNT(*)
                FROM chunks c
                INNER JOIN documents d ON d.id = c.document_id
                WHERE REPLACE(d.path, CHAR(92), '/') = ?
                   OR REPLACE(d.path, CHAR(92), '/') LIKE ?
                "#,
            )
            .bind(normalized_path)
            .bind(&prefix)
            .fetch_one(&self.pool)
            .await? as usize;
            (docs, chunks)
        };

        let status = if docs == 0 { "empty" } else { "indexed" };
        self.set_index_dir_status(path, docs, chunks, status).await
    }

    pub(crate) async fn failed_items(&self) -> Result<Vec<FailedFileView>, sqlx::Error> {
        let rows = sqlx::query_as::<_, FailedFileRow>(
            r#"
            SELECT file, reason, category, code, retry_count, last_failed_at
            FROM failed_files
            ORDER BY category, file
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| FailedFileView {
                file: row.file,
                reason: row.reason,
                category: row.category,
                code: row.code,
                retry_count: row.retry_count.max(0) as usize,
                last_failed_at: format_unix_ts(row.last_failed_at),
            })
            .collect())
    }

    pub(crate) async fn current_task(&self) -> Result<Option<CurrentTaskView>, sqlx::Error> {
        let row = sqlx::query_as::<_, CurrentTaskRow>(
            r#"
            SELECT label, details, state, current_dir, current_file, started_at, progress, scanned, total, succeeded, failed, updated, skipped, deleted, warning, pause_requested
            FROM current_task
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|row| CurrentTaskView {
            label: row.label,
            details: row.details,
            state: row.state,
            current_dir: row.current_dir,
            current_file: row.current_file,
            started_at: row.started_at,
            progress: row.progress as u8,
            scanned: row.scanned as usize,
            total: row.total as usize,
            succeeded: row.succeeded as usize,
            failed: row.failed as usize,
            updated: row.updated as usize,
            skipped: row.skipped as usize,
            deleted: row.deleted as usize,
            warning: row.warning.and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),
            pause_requested: row.pause_requested != 0,
        }))
    }
}
