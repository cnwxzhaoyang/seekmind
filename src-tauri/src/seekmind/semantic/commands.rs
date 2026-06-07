use std::sync::Arc;

use crate::seekmind::models::{
    SemanticDebugView, SemanticModelStatusView, SemanticRebuildProgressView,
    SemanticRebuildStartView,
};
use crate::seekmind::semantic::store;
use crate::seekmind::storage::Database;
use tauri::Emitter;

#[tauri::command]
pub async fn get_embedding_model_status(
    state: tauri::State<'_, Database>,
) -> Result<SemanticModelStatusView, String> {
    store::get_embedding_model_status(&state).await
}

#[tauri::command]
pub async fn list_embedding_models(
    state: tauri::State<'_, Database>,
) -> Result<Vec<crate::seekmind::models::EmbeddingModelView>, String> {
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
    app: tauri::AppHandle,
    state: tauri::State<'_, Database>,
) -> Result<SemanticRebuildStartView, String> {
    eprintln!("[SeekMind] rebuild_semantic_embeddings start");
    let job_id = uuid::Uuid::new_v4().to_string();
    let database = state.inner().clone();
    let start_status = store::get_embedding_model_status(&state).await?;
    let failure_model = start_status.model.clone();
    let emit_app = app.clone();
    let progress_emitter: Arc<dyn Fn(SemanticRebuildProgressView) + Send + Sync> =
        Arc::new(move |payload: SemanticRebuildProgressView| {
            let _ = emit_app.emit("seekmind:semantic:rebuild-progress", payload);
        });

    let task_job_id = job_id.clone();
    let task_app = app.clone();
    let initial_payload = SemanticRebuildProgressView {
        job_id: job_id.clone(),
        state: "running".to_string(),
        message: "正在准备语义模型".to_string(),
        source: "rebuild".to_string(),
        model: start_status.model.clone(),
        total_chunks: start_status.sqlite_chunks,
        processed_chunks: 0,
        embedded_chunks: 0,
        current_document: String::new(),
        current_chunk: String::new(),
        percent: 0,
        last_error: String::new(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let _ = app.emit("seekmind:semantic:rebuild-progress", initial_payload);

    tauri::async_runtime::spawn(async move {
        let result =
            store::rebuild_all_embeddings(&database, task_job_id.clone(), Some(progress_emitter))
                .await;
        match result {
            Ok(status) => {
                eprintln!(
                    "[SeekMind] rebuild_semantic_embeddings ok chunks={} embedded={} needs_rebuild={}",
                    status.sqlite_chunks, status.embedded_chunks, status.needs_rebuild
                );
            }
            Err(error) => {
                eprintln!("[SeekMind] rebuild_semantic_embeddings error: {error}");
                let _ = task_app.emit(
                    "seekmind:semantic:rebuild-progress",
                    SemanticRebuildProgressView {
                        job_id: task_job_id,
                        state: "failed".to_string(),
                        message: "语义向量重建失败".to_string(),
                        source: "rebuild".to_string(),
                        model: failure_model,
                        total_chunks: 0,
                        processed_chunks: 0,
                        embedded_chunks: 0,
                        current_document: String::new(),
                        current_chunk: String::new(),
                        percent: 0,
                        last_error: error,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }
        }
    });

    Ok(SemanticRebuildStartView {
        job_id,
        status: start_status,
    })
}

#[tauri::command]
pub async fn get_semantic_debug_report(
    query: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<SemanticDebugView, String> {
    store::semantic_debug_report(&state, &query, limit).await
}
