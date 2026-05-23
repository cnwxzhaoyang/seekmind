use crate::docmind::models::{SemanticDebugView, SemanticModelStatusView};
use crate::docmind::semantic::store;
use crate::docmind::storage::Database;

#[tauri::command]
pub async fn get_embedding_model_status(
    state: tauri::State<'_, Database>,
) -> Result<SemanticModelStatusView, String> {
    store::get_embedding_model_status(&state).await
}

#[tauri::command]
pub async fn list_embedding_models(
    state: tauri::State<'_, Database>,
) -> Result<Vec<crate::docmind::models::EmbeddingModelView>, String> {
    store::list_embedding_models(&state).await
}

#[tauri::command]
pub async fn set_default_embedding_model(
    model_id: String,
    state: tauri::State<'_, Database>,
) -> Result<SemanticModelStatusView, String> {
    store::set_default_embedding_model(&state, &model_id).await
}

#[tauri::command]
pub async fn rebuild_semantic_embeddings(
    state: tauri::State<'_, Database>,
) -> Result<SemanticModelStatusView, String> {
    eprintln!("[DocMind] rebuild_semantic_embeddings start");
    let result = store::rebuild_all_embeddings(&state).await;
    match result {
        Ok(status) => {
            eprintln!(
                "[DocMind] rebuild_semantic_embeddings ok chunks={} embedded={} needs_rebuild={}",
                status.sqlite_chunks, status.embedded_chunks, status.needs_rebuild
            );
            Ok(status)
        }
        Err(error) => {
            eprintln!("[DocMind] rebuild_semantic_embeddings error: {error}");
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn get_semantic_debug_report(
    query: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<SemanticDebugView, String> {
    store::semantic_debug_report(&state, &query, limit).await
}
