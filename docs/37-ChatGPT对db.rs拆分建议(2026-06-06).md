**ChatGPT:**

我看了一下你贴的 `db.rs`，它已经不只是“数据库访问层”，里面同时放了连接初始化、schema/migration、全文检索编排、文档索引、任务状态、收藏/标签/集合、QA 配置与历史等多类职责。拆分时不要按“行数均分”，要按业务边界和依赖方向拆。

建议把 `db.rs` 拆成 **storage/database 模块群**，保留一个很薄的 `Database` 门面，其它按领域拆。现在这个文件里能看到 `Database` 同时持有 `SqlitePool`、`SearchIndex`、索引任务锁，并且包含大量 `impl Database` 方法，这说明它已经变成“上帝对象”了。

粘贴的文本 (1)

## 一、目标结构

可以先拆成这样：

```
src/docmind/storage/
├── database/
│   ├── mod.rs              // Database 结构体、open_or_init、pool、index_job_running
│   ├── schema.rs           // init_schema + ensure_xxx migration
│   ├── settings.rs         // index_settings / qa_settings / proxy / model_profiles
│   ├── dirs.rs             // index_dirs、目录统计、enabled_index_dir_paths
│   ├── documents.rs        // documents/chunks/blocks 存储与查询
│   ├── search.rs           // search_documents / build_search_results / snippet/highlight/rerank
│   ├── task.rs             // current_task / checkpoint / index_run_summary / failed_files
│   ├── qa.rs               // qa_history / qa_sessions / qa_messages
│   ├── collections.rs      // collections / collection_items
│   ├── tags.rs             // tags / item_tags
│   ├── favorites.rs        // favorites / recent_documents / recent_views
│   ├── rows.rs             // 所有 sqlx::FromRow 结构体
│   ├── convert.rs          // row_to_view 转换函数
│   └── util.rs             // 时间、路径、normalize、scalar_count 等工具函数
```

然后原来的 `db.rs` 变成兼容入口：

```Rust
pub mod database;

pub use database::{
    Database,
    sqlite_database_path,
    SearchDebugData,
    IndexCheckpointRow,
};
```

这样外部如果还在用 `crate::docmind::storage::db::Database`，你可以短期保留 `db.rs` 做 re-export，后续再统一改引用。

---

## 二、按职责拆的理由

### 1. `mod.rs`：只保留核心门面

现在 `Database` 本身只需要保留状态字段和极少量基础方法：

```Rust
#[derive(Clone)]
pub struct Database {
    pub(crate) pool: SqlitePool,
    pub(crate) search_index: Arc<SearchIndex>,
    pub(crate) index_job_running: Arc<AtomicBool>,
}
```

注意字段建议改成 `pub(crate)`，否则子模块里的 `impl Database` 访问不到。

`mod.rs` 可以放：

```Rust
mod schema;
mod settings;
mod dirs;
mod documents;
mod search;
mod task;
mod qa;
mod collections;
mod tags;
mod favorites;
mod rows;
mod convert;
mod util;

pub use search::SearchDebugData;
pub use task::IndexCheckpointRow;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::docmind::storage::fulltext::SearchIndex;

#[derive(Clone)]
pub struct Database {
    pub(crate) pool: SqlitePool,
    pub(crate) search_index: Arc<SearchIndex>,
    pub(crate) index_job_running: Arc<AtomicBool>,
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

    pub async fn open_or_init() -> Result<Self, String> {
        // 原 open_or_init 放这里
        // 但 init_schema / ensure_xxx 调用 schema.rs 里的方法
        todo!()
    }
}
```

---

## 三、第一轮最值得先拆的 4 块

不要一次性全部拆完。建议第一轮先拆这几个，收益最大、风险较低。

### 1. `rows.rs`

你现在顶部有大量 `sqlx::FromRow` 结构体，例如 `IndexDirRow`、`DocumentRow`、`ChunkRow`、`QaSettingsRow`、`CollectionRow` 等。

粘贴的文本 (1)

直接挪到：

```Rust
// database/rows.rs

use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub(crate) struct IndexDirRow {
    pub(crate) path: String,
    pub(crate) enabled: i64,
    pub(crate) docs: i64,
    pub(crate) chunks: i64,
    pub(crate) status: String,
}

// 其他 Row 同理
```

这一块拆出来基本不改变逻辑，最安全。

---

### 2. `schema.rs`

`init_schema()` 和所有 `ensure_xxx_columns()` 都应该独立出去。现在 `open_or_init()` 里连续调用大量 schema/migration 方法，比如 `init_schema`、`ensure_documents_columns`、`ensure_current_task_columns`、`ensure_failed_files_columns` 等。

粘贴的文本 (1)

拆成：

```Rust
// database/schema.rs

use super::Database;

impl Database {
    pub(crate) async fn init_schema(&self) -> Result<(), sqlx::Error> {
        // 原 init_schema
        todo!()
    }

    pub(crate) async fn ensure_documents_columns(&self) -> Result<bool, sqlx::Error> {
        todo!()
    }

    pub(crate) async fn ensure_current_task_columns(&self) -> Result<(), sqlx::Error> {
        todo!()
    }

    // 其他 ensure_xxx
}
```

这部分属于数据库结构维护，和业务查询逻辑完全不同，应该单独放。

---

### 3. `search.rs`

全文检索、语义检索、重排、摘要、高亮这些应该独立。现在 `build_search_results()` 里同时做：

```Rust
let settings = self.get_index_settings().await?;
let keyword_hits = self.search_index.search(...);
let semantic_result = semantic_store::semantic_search_hits(...);
let rows = self.fetch_chunks_by_ids(&chunk_ids).await?;
let mut preview_blocks_by_chunk_id = self.fetch_preview_blocks_for_search_rows(&rows).await?;
```

这说明它是一个“搜索编排器”，不应该和 schema、收藏、QA、集合混在一起。

粘贴的文本 (1)

建议放：

```Rust
// database/search.rs

use super::Database;

#[derive(Debug, Clone)]
pub(crate) struct SearchDebugData {
    // 原 SearchDebugData
}

impl Database {
    pub async fn search_documents(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResultView>, sqlx::Error> {
        Ok(self.build_search_results(query, limit).await?.hits)
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
        // 原逻辑
        todo!()
    }
}

// search_rank_reason / build_search_snippet / highlight_spans 等也放这里
```

---

### 4. `documents.rs`

文档、chunk、block、OCR 任务的存储和查询应该是一组。比如 `store_document()` 同时写入 `documents`、`document_blocks`、`chunks`、`pdf_ocr_tasks`，再写 Tantivy 全文索引。

粘贴的文本 (1)

放到：

```Rust
// database/documents.rs

impl Database {
    pub async fn list_document_chunks(&self, path: &str) -> Result<Vec<ChunkView>, sqlx::Error> {
        todo!()
    }

    pub(crate) async fn store_document(
        &self,
        document: &ExtractedDocument,
        chunks: &[ChunkRecord],
        blocks: &[ParsedBlock],
        ocr_tasks: &[PdfOcrTask],
    ) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub(crate) async fn clear_document_by_path(&self, path: &str) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub(crate) async fn document_id_by_path(
        &self,
        path: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        todo!()
    }
}
```

---

## 四、建议的拆分顺序

我建议按这个顺序来，避免一次重构炸掉：

### 第一步：只拆 `rows.rs`、`util.rs`

先搬纯结构体和纯函数。

例如这些函数可以先放 `util.rs`：

```Rust
current_unix_ts()
format_unix_ts()
database_path()
sqlite_database_path()
normalize_directory_path()
is_virtual_directory()
is_path_within_dir()
default_exclude_dirs()
scalar_count_no_bind()
scalar_count_bind()
truncate_by_chars()
slice_chars()
```

但是 `build_search_snippet()`、`highlight_spans()` 更偏搜索，建议放 `search.rs`，不要放通用 util。

---

### 第二步：拆 `schema.rs`

把 `init_schema()` 和 `ensure_xxx()` 全部挪走。

这一步改完后，`open_or_init()` 仍然在 `mod.rs`，只是调用子模块里的方法。

---

### 第三步：拆业务模块

按这个顺序拆：

```
settings.rs
dirs.rs
documents.rs
task.rs
search.rs
qa.rs
favorites.rs
tags.rs
collections.rs
```

原因是 `search.rs` 依赖 settings、favorites、recent、documents 的一些方法，所以最好不要第一个拆它。

---

## 五、最终 `mod.rs` 应该长这样

```Rust
mod collections;
mod convert;
mod dirs;
mod documents;
mod favorites;
mod qa;
mod rows;
mod schema;
mod search;
mod settings;
mod tags;
mod task;
mod util;

pub use search::SearchDebugData;
pub use task::IndexCheckpointRow;
pub use util::sqlite_database_path;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::docmind::storage::fulltext::SearchIndex;

#[derive(Clone)]
pub struct Database {
    pub(crate) pool: SqlitePool,
    pub(crate) search_index: Arc<SearchIndex>,
    pub(crate) index_job_running: Arc<AtomicBool>,
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

    pub async fn open_or_init() -> Result<Self, String> {
        let path = util::database_path();
        eprintln!("[DocMind] SQLite database path: {}", path.display());

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

        let search_index = Arc::new(SearchIndex::open_or_init()?);

        let database = Self {
            pool,
            search_index,
            index_job_running: Arc::new(AtomicBool::new(false)),
        };

        database.init_schema().await.map_err(|e| e.to_string())?;

        let documents_migrated = database
            .ensure_documents_columns()
            .await
            .map_err(|e| e.to_string())?;

        database.ensure_current_task_columns().await.map_err(|e| e.to_string())?;
        database.ensure_failed_files_columns().await.map_err(|e| e.to_string())?;
        database.ensure_index_settings_columns().await.map_err(|e| e.to_string())?;
        database.ensure_index_settings_row().await.map_err(|e| e.to_string())?;
        database.ensure_index_run_summary_row().await.map_err(|e| e.to_string())?;
        database.ensure_index_checkpoint_table().await.map_err(|e| e.to_string())?;
        database.ensure_embedding_models_row().await.map_err(|e| e.to_string())?;
        database.ensure_vector_index_meta_row().await.map_err(|e| e.to_string())?;
        database.ensure_history_tables().await.map_err(|e| e.to_string())?;
        database.ensure_qa_settings_row().await.map_err(|e| e.to_string())?;
        database.ensure_qa_history_columns().await.map_err(|e| e.to_string())?;
        database.ensure_network_proxy_settings_row().await.map_err(|e| e.to_string())?;
        database.ensure_collections_seed().await.map_err(|e| e.to_string())?;
        database.ensure_qa_model_profiles_row().await.map_err(|e| e.to_string())?;
        database.ensure_chunks_block_indexes_column().await.map_err(|e| e.to_string())?;
        database.ensure_document_blocks_columns().await.map_err(|e| e.to_string())?;

        if documents_migrated {
            database.clear_all_index_data().await.map_err(|e| e.to_string())?;
        }

        Ok(database)
    }
}
```

---

## 六、注意一个 Rust 拆分坑

你现在很多方法是：

```Rust
impl Database {
    async fn xxx(&self) -> ...
}
```

拆到多个文件后，只要这些文件都在同一个模块树里，并且 `Database` 的字段是 `pub(crate)`，就可以继续写：

```Rust
use super::Database;

impl Database {
    pub(crate) async fn xxx(&self) -> ... {
        ...
    }
}
```

不需要 trait，不需要把 `Database` 拆成多个 struct。

---

## 七、什么时候再进一步拆 Repository？

第一阶段不要急着搞复杂的 repository trait。你现在更适合先做 **物理文件拆分**，保持调用方式不变。

也就是外部仍然这样用：

```Rust
database.search_documents(...)
database.store_document(...)
database.list_qa_sessions(...)
```

只是内部代码分散到不同文件。

等后面稳定后，再考虑拆成：

```Rust
DocumentRepository
SearchRepository
QaRepository
CollectionRepository
```

但这会影响调用方式，重构成本更高。

---

## 八、我的建议结论

你这个 `db.rs` 最合理的拆法是：

```
先按领域拆 impl Database，不急着拆 Database 类型。
```

第一轮优先拆：

```
rows.rs
util.rs
schema.rs
settings.rs
documents.rs
search.rs
task.rs
qa.rs
collections.rs
tags.rs
favorites.rs
```

拆完后，`db.rs/mod.rs` 控制在 150～250 行左右，其它每个模块控制在 300～800 行。这样比现在一个 5000 多行文件更容易维护，而且不会大规模破坏现有调用链。