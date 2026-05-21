use super::data;
use super::file_ops;
use super::models::{CurrentTaskView, IndexStatusView};
use super::search;

#[tauri::command]
pub fn list_index_dirs() -> Vec<super::models::IndexDirView> {
    data::index_dirs()
}

#[tauri::command]
pub fn search_documents(query: String, limit: usize) -> Vec<super::models::SearchResultView> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let query_terms = search::normalize_query(query);
    let results = data::search_results();

    let mut matched = search::filter_results(&query_terms, &results);
    if matched.is_empty() {
        matched = search::relax_results(&query_terms, &results);
    }

    search::sort_results(matched)
        .into_iter()
        .take(limit.max(1))
        .collect()
}

#[tauri::command]
pub fn get_index_status() -> IndexStatusView {
    let failed_items = data::failed_files();

    IndexStatusView {
        indexed_docs: 1_038,
        indexed_chunks: 16_124,
        scanned_docs: 1_128,
        failed_files: failed_items.len(),
        current_task: Some(CurrentTaskView {
            label: "正在解析 Downloads 目录中的新文档".to_string(),
            details: "后台正在扫描新增或变更文件".to_string(),
            progress: 68,
            scanned: 768,
            total: 1_128,
        }),
        failed_items,
    }
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    file_ops::open_file_path(&path)
}
