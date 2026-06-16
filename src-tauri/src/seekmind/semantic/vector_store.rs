/**
 * @author MorningSun
 * @CreatedDate 2026/06/16
 * @Description SeekMind 语义向量存储后端选择与运行时切换。
 */
use std::sync::OnceLock;

use libsqlite3_sys::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VectorStoreBackend {
    LegacyJson,
    SqliteVec,
    LanceDb,
}

impl VectorStoreBackend {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LegacyJson => "legacy-json",
            Self::SqliteVec => "sqlite-vec",
            Self::LanceDb => "lancedb",
        }
    }
}

pub(crate) fn resolve_vector_store_backend() -> VectorStoreBackend {
    configured_vector_store_backend()
}

pub(crate) fn ensure_sqlite_vec_registered() -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            // 修复：sqlite-vec 通过 SQLite auto extension 注册，必须在连接池建立前完成。
            unsafe {
                sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
            }
            eprintln!("[SeekMind] sqlite-vec extension registered");
            Ok(())
        })
        .clone()
}

fn configured_vector_store_backend() -> VectorStoreBackend {
    let value = std::env::var("SEEKMIND_VECTOR_STORE")
        .unwrap_or_else(|_| "sqlite-vec".to_string())
        .trim()
        .to_lowercase();
    match value.as_str() {
        "" | "sqlite-vec" | "sqlitevec" => VectorStoreBackend::SqliteVec,
        "legacy" | "legacy-json" | "json" | "sqlite-json" => VectorStoreBackend::LegacyJson,
        "lancedb" | "lance" => VectorStoreBackend::LanceDb,
        _ => VectorStoreBackend::SqliteVec,
    }
}
