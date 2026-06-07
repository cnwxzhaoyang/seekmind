/**
 * @author MorningSun
 * @CreatedDate 2026/06/05
 * @Description SeekMind 数据库通用工具函数。
 */

use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use dirs::data_dir;
use sqlx::sqlite::SqlitePool;
use sqlx::Row;

fn database_path() -> PathBuf {
    let base = data_dir().unwrap_or_else(|| PathBuf::from("."));
    #[cfg(debug_assertions)]
    {
        return base.join("SeekMindDev").join("seekmind.sqlite");
    }

    #[cfg(not(debug_assertions))]
    {
        base.join("SeekMind").join("seekmind.sqlite")
    }
}

pub fn sqlite_database_path() -> PathBuf {
    database_path()
}

pub(crate) fn normalize_directory_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

pub(crate) fn is_virtual_directory(path: &str) -> bool {
    normalize_directory_path(path).starts_with("virtual://")
}

pub(crate) fn is_path_within_dir(path: &str, dir: &str) -> bool {
    let normalized_path = normalize_directory_path(path);
    let normalized_dir = normalize_directory_path(dir);
    if normalized_path == normalized_dir {
        return true;
    }

    normalized_path.starts_with(&format!("{normalized_dir}/"))
}

pub(crate) fn default_exclude_dirs() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        ".git".to_string(),
        "target".to_string(),
        "Library".to_string(),
        "Caches".to_string(),
        "Application Support".to_string(),
    ]
}

pub(crate) fn current_unix_ts() -> i64 {
    Utc::now().timestamp()
}

pub(crate) fn format_unix_ts(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "未知".to_string();
    }

    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "未知".to_string())
}

pub(crate) async fn scalar_count_no_bind(
    pool: &SqlitePool,
    sql: &str,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(sql).fetch_one(pool).await?;
    row.try_get::<i64, _>(0)
}

pub(crate) async fn scalar_count_bind(
    pool: &SqlitePool,
    sql: &str,
    bind: &str,
) -> Result<i64, sqlx::Error> {
    let row = sqlx::query(sql).bind(bind).fetch_one(pool).await?;
    row.try_get::<i64, _>(0)
}
