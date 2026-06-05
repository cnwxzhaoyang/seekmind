/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description DocMind SQLite schema / migration 逻辑。
 */

use uuid::Uuid;

use crate::docmind::storage::types::IndexSettings;

use super::util::{current_unix_ts, default_exclude_dirs, scalar_count_no_bind};
use super::{default_network_proxy_settings, default_qa_settings, Database};
use sqlx::Row;

impl Database {
    pub(crate) async fn init_schema(&self) -> Result<(), sqlx::Error> {
        let statements = [
            r#"
            CREATE TABLE IF NOT EXISTS index_dirs (
                path TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                docs INTEGER NOT NULL DEFAULT 0,
                chunks INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                dir_path TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                file_name TEXT NOT NULL,
                ext TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                modified_at INTEGER NOT NULL DEFAULT 0,
                content_hash TEXT NOT NULL DEFAULT '',
                modified TEXT NOT NULL,
                content TEXT NOT NULL,
                FOREIGN KEY(dir_path) REFERENCES index_dirs(path) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                heading TEXT NOT NULL,
                snippet TEXT NOT NULL,
                paragraph INTEGER,
                page INTEGER,
                score REAL NOT NULL,
                block_indexes_json TEXT NOT NULL DEFAULT '[]',
                FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS document_blocks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                block_index INTEGER NOT NULL,
                block_type TEXT NOT NULL,
                text TEXT NOT NULL,
                heading TEXT NOT NULL DEFAULT '',
                level INTEGER,
                page INTEGER,
                language TEXT NOT NULL DEFAULT '',
                markdown TEXT NOT NULL DEFAULT '',
                html TEXT NOT NULL DEFAULT '',
                asset_path TEXT NOT NULL DEFAULT '',
                alt_text TEXT NOT NULL DEFAULT '',
                caption TEXT NOT NULL DEFAULT '',
                ocr_text TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS pdf_ocr_tasks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                document_path TEXT NOT NULL DEFAULT '',
                page_index INTEGER NOT NULL,
                reason TEXT NOT NULL DEFAULT '',
                message TEXT NOT NULL DEFAULT '',
                warning TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'queued',
                ocr_text TEXT NOT NULL DEFAULT '',
                error TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(document_id) REFERENCES documents(id) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS failed_files (
                file TEXT PRIMARY KEY,
                reason TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'unknown',
                code TEXT NOT NULL DEFAULT 'unknown',
                retry_count INTEGER NOT NULL DEFAULT 0,
                first_failed_at INTEGER NOT NULL DEFAULT 0,
                last_failed_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS current_task (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                label TEXT NOT NULL,
                details TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'idle',
                current_dir TEXT NOT NULL DEFAULT '',
                current_file TEXT NOT NULL DEFAULT '',
                started_at INTEGER NOT NULL DEFAULT 0,
                progress INTEGER NOT NULL,
                scanned INTEGER NOT NULL,
                total INTEGER NOT NULL,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0,
                warning TEXT NOT NULL DEFAULT '',
                pause_requested INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS index_run_summary (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0,
                scanned INTEGER NOT NULL DEFAULT 0,
                total INTEGER NOT NULL DEFAULT 0,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                completed_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS index_checkpoint (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                dir_paths TEXT NOT NULL,
                pending_delete_paths TEXT NOT NULL,
                pending_update_paths TEXT NOT NULL,
                phase TEXT NOT NULL,
                current_dir TEXT NOT NULL DEFAULT '',
                current_file TEXT NOT NULL DEFAULT '',
                total INTEGER NOT NULL DEFAULT 0,
                processed INTEGER NOT NULL DEFAULT 0,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS index_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                exclude_dirs TEXT NOT NULL,
                exclude_exts TEXT NOT NULL,
                max_file_size_mb INTEGER NOT NULL,
                semantic_search_enabled INTEGER NOT NULL DEFAULT 1,
                semantic_weight REAL NOT NULL DEFAULT 0.25,
                semantic_threshold REAL NOT NULL DEFAULT 0.2,
                title_weight REAL NOT NULL DEFAULT 1,
                filename_weight REAL NOT NULL DEFAULT 1,
                preference_weight REAL NOT NULL DEFAULT 1,
                prefer_favorites_enabled INTEGER NOT NULL DEFAULT 1,
                prefer_recent_enabled INTEGER NOT NULL DEFAULT 1,
                prefer_history_enabled INTEGER NOT NULL DEFAULT 1
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS embedding_models (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                model_path TEXT NOT NULL DEFAULT '',
                dimension INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                available INTEGER NOT NULL DEFAULT 0,
                is_default INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'unknown',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS chunk_embeddings (
                chunk_id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                model_id TEXT NOT NULL,
                vector_json TEXT NOT NULL,
                dimension INTEGER NOT NULL,
                text_hash TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'ready',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS vector_index_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                model_id TEXT NOT NULL,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                last_indexed_at INTEGER NOT NULL DEFAULT 0,
                last_error TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'idle'
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS search_history (
                query TEXT PRIMARY KEY,
                normalized_query TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                last_hit_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recent_documents (
                path TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                file_name TEXT NOT NULL,
                ext TEXT NOT NULL,
                last_opened_at INTEGER NOT NULL DEFAULT 0,
                open_count INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recent_views (
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                title TEXT NOT NULL,
                path TEXT NOT NULL DEFAULT '',
                viewed_at INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (target_type, target_id)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS item_tags (
                id TEXT PRIMARY KEY,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(tag_id) REFERENCES tags(id) ON DELETE CASCADE,
                UNIQUE(target_type, target_id, tag_id)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS favorites (
                target TEXT PRIMARY KEY,
                favorite_type TEXT NOT NULL,
                title TEXT NOT NULL,
                path TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                color TEXT NOT NULL DEFAULT '',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS collection_items (
                id TEXT PRIMARY KEY,
                collection_id TEXT NOT NULL,
                item_type TEXT NOT NULL,
                document_id TEXT NOT NULL DEFAULT '',
                chunk_id TEXT NOT NULL DEFAULT '',
                qa_session_id TEXT NOT NULL DEFAULT '',
                qa_message_id TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                path TEXT NOT NULL DEFAULT '',
                title_path TEXT NOT NULL DEFAULT '',
                snippet TEXT NOT NULL DEFAULT '',
                note TEXT NOT NULL DEFAULT '',
                source_meta_json TEXT NOT NULL DEFAULT '{}',
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(collection_id) REFERENCES collections(id) ON DELETE CASCADE
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                provider TEXT NOT NULL DEFAULT 'openai_compatible',
                base_url TEXT NOT NULL DEFAULT '',
                api_key TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                temperature REAL NOT NULL DEFAULT 0.2,
                max_output_tokens INTEGER NOT NULL DEFAULT 600,
                context_chunk_limit INTEGER NOT NULL DEFAULT 8,
                context_token_budget INTEGER NOT NULL DEFAULT 6000,
                min_evidence_count INTEGER NOT NULL DEFAULT 2,
                min_retrieval_score REAL NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS network_proxy_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                proxy_url TEXT NOT NULL DEFAULT '',
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_model_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT NOT NULL DEFAULT '',
                api_key TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_history (
                id TEXT PRIMARY KEY,
                question TEXT NOT NULL,
                answer TEXT NOT NULL,
                state TEXT NOT NULL,
                sources_json TEXT NOT NULL DEFAULT '[]',
                retrieval_json TEXT NOT NULL DEFAULT '{}',
                model TEXT NOT NULL DEFAULT '',
                error TEXT NOT NULL DEFAULT '',
                warning TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS qa_messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                question TEXT NOT NULL,
                answer TEXT NOT NULL,
                state TEXT NOT NULL,
                sources_json TEXT NOT NULL DEFAULT '[]',
                retrieval_json TEXT NOT NULL DEFAULT '{}',
                model TEXT NOT NULL DEFAULT '',
                error TEXT NOT NULL DEFAULT '',
                warning TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(session_id) REFERENCES qa_sessions(id) ON DELETE CASCADE
            )
            "#,
        ];

        for statement in statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    pub(crate) async fn ensure_index_settings_row(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM index_settings").await?;
        if count == 0 {
            let defaults = IndexSettings {
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
            };
            self.save_index_settings(&defaults).await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_qa_settings_row(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM qa_settings").await?;
        if count == 0 {
            self.save_qa_settings(&default_qa_settings()).await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_qa_history_columns(&self) -> Result<(), sqlx::Error> {
        let mut history_columns = std::collections::HashSet::new();
        for row in sqlx::query("PRAGMA table_info(qa_history)")
            .fetch_all(&self.pool)
            .await?
        {
            let name: String = row.try_get("name")?;
            history_columns.insert(name);
        }

        if !history_columns.contains("warning") {
            eprintln!("[DocMind] migrate qa_history.warning column");
            sqlx::query("ALTER TABLE qa_history ADD COLUMN warning TEXT NOT NULL DEFAULT ''")
                .execute(&self.pool)
                .await?;
        }

        let mut message_columns = std::collections::HashSet::new();
        for row in sqlx::query("PRAGMA table_info(qa_messages)")
            .fetch_all(&self.pool)
            .await?
        {
            let name: String = row.try_get("name")?;
            message_columns.insert(name);
        }

        if !message_columns.contains("warning") {
            eprintln!("[DocMind] migrate qa_messages.warning column");
            sqlx::query("ALTER TABLE qa_messages ADD COLUMN warning TEXT NOT NULL DEFAULT ''")
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    pub(crate) async fn ensure_network_proxy_settings_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM network_proxy_settings").await?;
        if count == 0 {
            self.save_network_proxy_settings(&default_network_proxy_settings())
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_collections_seed(&self) -> Result<(), sqlx::Error> {
        let count = scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM collections").await?;
        if count == 0 {
            self.create_collection("默认主题集合", "").await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_qa_model_profiles_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM qa_model_profiles").await?;
        if count == 0 {
            let settings = self
                .get_qa_settings()
                .await
                .unwrap_or_else(|_| default_qa_settings());
            let now = current_unix_ts();
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO qa_model_profiles
                    (id, name, provider, base_url, api_key, model, enabled, is_default, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?)
                "#,
            )
            .bind(&id)
            .bind("默认连接")
            .bind(settings.provider)
            .bind(settings.base_url)
            .bind(settings.api_key)
            .bind(settings.model)
            .bind(settings.enabled as i64)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_embedding_models_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM embedding_models").await?;
        if count == 0 {
            let now = current_unix_ts();
            sqlx::query(
                r#"
                INSERT INTO embedding_models
                    (id, name, provider, model_path, dimension, enabled, available, is_default, status, created_at, updated_at)
                VALUES
                    ('default-local-embedding', 'BAAI/bge-small-zh-v1.5', 'fastembed', '', 512, 1, 0, 1, 'unknown', ?, ?)
                "#,
            )
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        } else {
            let now = current_unix_ts();
            sqlx::query(
                r#"
                UPDATE embedding_models
                SET provider = 'fastembed',
                    dimension = 512,
                    updated_at = ?
                WHERE name = 'BAAI/bge-small-zh-v1.5'
                "#,
            )
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_vector_index_meta_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM vector_index_meta").await?;
        if count == 0 {
            sqlx::query(
                r#"
                INSERT INTO vector_index_meta (id, model_id, chunk_count, last_indexed_at, last_error, status)
                VALUES (1, 'default-local-embedding', 0, 0, '', 'idle')
                "#,
            )
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_current_task_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(current_task)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("current_dir") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN current_dir TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("current_file") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN current_file TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("started_at") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN started_at INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("succeeded") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN succeeded INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("failed") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN failed INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("updated") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN updated INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("skipped") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN skipped INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("deleted") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("warning") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN warning TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("state") {
            alter_statements
                .push("ALTER TABLE current_task ADD COLUMN state TEXT NOT NULL DEFAULT 'idle'");
        }
        if !columns.contains("pause_requested") {
            alter_statements.push(
                "ALTER TABLE current_task ADD COLUMN pause_requested INTEGER NOT NULL DEFAULT 0",
            );
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    pub(crate) async fn ensure_failed_files_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(failed_files)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("category") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN category TEXT NOT NULL DEFAULT 'unknown'",
            );
        }
        if !columns.contains("code") {
            alter_statements
                .push("ALTER TABLE failed_files ADD COLUMN code TEXT NOT NULL DEFAULT 'unknown'");
        }
        if !columns.contains("retry_count") {
            alter_statements
                .push("ALTER TABLE failed_files ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0");
        }
        if !columns.contains("first_failed_at") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN first_failed_at INTEGER NOT NULL DEFAULT 0",
            );
        }
        if !columns.contains("last_failed_at") {
            alter_statements.push(
                "ALTER TABLE failed_files ADD COLUMN last_failed_at INTEGER NOT NULL DEFAULT 0",
            );
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    pub(crate) async fn ensure_index_run_summary_row(&self) -> Result<(), sqlx::Error> {
        let count =
            scalar_count_no_bind(&self.pool, "SELECT COUNT(*) FROM index_run_summary").await?;
        if count == 0 {
            self.save_index_run_summary(0, 0, 0, 0, 0, 0, 0).await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_index_settings_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(index_settings)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("semantic_search_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN semantic_search_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("semantic_weight") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN semantic_weight REAL NOT NULL DEFAULT 0.25",
            );
        }
        if !columns.contains("semantic_threshold") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN semantic_threshold REAL NOT NULL DEFAULT 0.2",
            );
        }
        if !columns.contains("title_weight") {
            alter_statements
                .push("ALTER TABLE index_settings ADD COLUMN title_weight REAL NOT NULL DEFAULT 1");
        }
        if !columns.contains("filename_weight") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN filename_weight REAL NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("preference_weight") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN preference_weight REAL NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("prefer_favorites_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN prefer_favorites_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("prefer_recent_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN prefer_recent_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }
        if !columns.contains("prefer_history_enabled") {
            alter_statements.push(
                "ALTER TABLE index_settings ADD COLUMN prefer_history_enabled INTEGER NOT NULL DEFAULT 1",
            );
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }

    pub(crate) async fn ensure_index_checkpoint_table(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS index_checkpoint (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                dir_paths TEXT NOT NULL,
                pending_delete_paths TEXT NOT NULL,
                pending_update_paths TEXT NOT NULL,
                phase TEXT NOT NULL,
                current_dir TEXT NOT NULL DEFAULT '',
                current_file TEXT NOT NULL DEFAULT '',
                total INTEGER NOT NULL DEFAULT 0,
                processed INTEGER NOT NULL DEFAULT 0,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                updated INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                deleted INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn ensure_history_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS search_history (
                query TEXT PRIMARY KEY,
                normalized_query TEXT NOT NULL,
                hit_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0,
                last_hit_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS recent_documents (
                path TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                file_name TEXT NOT NULL,
                ext TEXT NOT NULL,
                last_opened_at INTEGER NOT NULL DEFAULT 0,
                open_count INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS favorites (
                target TEXT PRIMARY KEY,
                favorite_type TEXT NOT NULL,
                title TEXT NOT NULL,
                path TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub(crate) async fn ensure_documents_columns(&self) -> Result<bool, sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(documents)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut altered = false;
        let mut alter_statements = Vec::new();
        if !columns.contains("dir_path") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN dir_path TEXT NOT NULL DEFAULT ''");
            altered = true;
        }
        if !columns.contains("file_size") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0");
            altered = true;
        }
        if !columns.contains("modified_at") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN modified_at INTEGER NOT NULL DEFAULT 0");
            altered = true;
        }
        if !columns.contains("content_hash") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN content_hash TEXT NOT NULL DEFAULT ''");
            altered = true;
        }
        if !columns.contains("content") {
            alter_statements
                .push("ALTER TABLE documents ADD COLUMN content TEXT NOT NULL DEFAULT ''");
            altered = true;
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(altered)
    }

    pub(crate) async fn ensure_chunks_block_indexes_column(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(chunks)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        if !columns.contains("block_indexes_json") {
            sqlx::query(
                r#"
                ALTER TABLE chunks ADD COLUMN block_indexes_json TEXT NOT NULL DEFAULT '[]'
                "#,
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub(crate) async fn ensure_document_blocks_columns(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query("PRAGMA table_info(document_blocks)")
            .fetch_all(&self.pool)
            .await?;

        let mut columns = std::collections::HashSet::new();
        for row in existing {
            let name: String = row.try_get("name")?;
            columns.insert(name);
        }

        let mut alter_statements = Vec::new();
        if !columns.contains("asset_path") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN asset_path TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("alt_text") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN alt_text TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("caption") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN caption TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("language") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN language TEXT NOT NULL DEFAULT ''");
        }
        if !columns.contains("ocr_text") {
            alter_statements
                .push("ALTER TABLE document_blocks ADD COLUMN ocr_text TEXT NOT NULL DEFAULT ''");
        }

        for statement in alter_statements {
            sqlx::query(statement).execute(&self.pool).await?;
        }

        Ok(())
    }
}
