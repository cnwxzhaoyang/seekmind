use crate::docmind::models::{
    QaAnswerProgressView, QaAnswerView, QaAskStartView, QaConnectionTestView, QaHistoryView,
    QaMessageView, QaRetrievalView, QaSessionView, QaSettingsView, QaSourceView,
};
use crate::docmind::storage::db::Database;
use crate::docmind::storage::types::QaSettings;
use crate::docmind::search::normalize_query;

use super::cancel::{cancel as cancel_qa_job, clear as clear_qa_job, register as register_qa_job};
use super::client::{ask_model, ask_model_stream};
use super::context::build_qa_context;
use super::models::QaContext;
use futures_util::future::{AbortHandle, Abortable};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

fn qa_settings_to_view(settings: &QaSettings) -> QaSettingsView {
    QaSettingsView {
        enabled: settings.enabled,
        provider: settings.provider.clone(),
        base_url: settings.base_url.clone(),
        api_key: settings.api_key.clone(),
        model: settings.model.clone(),
        temperature: settings.temperature,
        max_output_tokens: settings.max_output_tokens,
        context_chunk_limit: settings.context_chunk_limit,
        context_token_budget: settings.context_token_budget,
        min_evidence_count: settings.min_evidence_count,
        min_retrieval_score: settings.min_retrieval_score,
        updated_at: String::new(),
    }
}

fn build_system_prompt(context: &QaContext, session_context: Option<&str>) -> String {
    let mut prompt = String::from(
        "你是 DocMind 的本地文档问答引擎。只能基于给定来源回答，不能使用外部知识补全事实。\
如果来源不足，直接说明无法从当前文档判断。回答要简短、具体、可回溯。\
不要编造新的来源编号，不要输出与来源无关的结论。\
如果能回答，请用与用户问题相同的语言输出，并尽量把结论控制在 3 到 6 句以内。\n\n可用来源如下：\n",
    );

    if let Some(session_context) = session_context.filter(|text| !text.trim().is_empty()) {
        prompt.push_str("\n最近对话上下文（仅用于理解指代，不可当作事实来源）：\n");
        prompt.push_str(session_context.trim());
        prompt.push_str("\n\n");
    }

    for source in &context.sources {
        prompt.push('\n');
        prompt.push_str(&source.block);
        prompt.push_str("\n\n");
    }

    prompt.push_str(
        "输出要求：\n\
1. 只输出最终答案正文，不要输出 JSON。\n\
2. 不要列出你没有使用的来源。\n\
3. 如果无法回答，直接说明证据不足，并说明建议补充哪些文档类型或关键词。\n",
    );

    prompt
}

fn truncate_prompt_text(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn build_session_context(messages: &[QaMessageView]) -> String {
    let mut lines = Vec::new();
    for message in messages.iter().rev().take(3).rev() {
        let question = truncate_prompt_text(message.question.trim(), 80);
        let answer = truncate_prompt_text(message.answer.trim(), 140);
        if question.is_empty() && answer.is_empty() {
            continue;
        }

        lines.push(format!("Q: {question}"));
        if !answer.is_empty() {
            lines.push(format!("A: {answer}"));
        }
    }

    lines.join("\n")
}

fn build_session_terms(messages: &[QaMessageView], current_terms: &[String]) -> Vec<String> {
    let current_set = current_terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<std::collections::HashSet<_>>();
    let stop_terms = [
        "什么", "怎么", "如何", "是否", "这个", "那个", "它的", "它", "以及", "问题", "答案", "来源", "文档", "内容", "可以", "已经",
    ]
    .into_iter()
    .collect::<std::collections::HashSet<_>>();

    let mut terms = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();

    for message in messages.iter().rev().take(3) {
        let tokens = normalize_query(&format!("{} {}", message.question, message.answer));
        for token in tokens {
            let normalized = token.trim().to_lowercase();
            if normalized.is_empty()
                || normalized.len() < 2
                || current_set.contains(&normalized)
                || stop_terms.contains(normalized.as_str())
                || seen.contains(&normalized)
            {
                continue;
            }

            seen.insert(normalized.clone());
            terms.push(normalized);
            if terms.len() >= 6 {
                return terms;
            }
        }
    }

    terms
}

fn build_answer_view(
    id: String,
    question: String,
    answer: String,
    state: String,
    sources: Vec<crate::docmind::models::QaSourceView>,
    retrieval: crate::docmind::models::QaRetrievalView,
    model: String,
    created_at: String,
    error: Option<String>,
) -> QaAnswerView {
    QaAnswerView {
        id,
        question,
        answer,
        state,
        sources,
        retrieval,
        model,
        created_at,
        error,
    }
}

fn build_progress_view(
    job_id: String,
    state: String,
    question: String,
    answer: String,
    sources: Vec<crate::docmind::models::QaSourceView>,
    retrieval: crate::docmind::models::QaRetrievalView,
    model: String,
    error: Option<String>,
) -> QaAnswerProgressView {
    QaAnswerProgressView {
        job_id,
        state,
        question,
        answer,
        sources,
        retrieval,
        model,
        error,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

#[tauri::command]
pub async fn cancel_qa_question(job_id: String) -> Result<(), String> {
    let _ = cancel_qa_job(job_id.trim());
    Ok(())
}

#[tauri::command]
pub async fn get_qa_settings(
    state: tauri::State<'_, Database>,
) -> Result<QaSettingsView, String> {
    let settings = state.get_qa_settings().await.map_err(|error| error.to_string())?;
    let mut view = qa_settings_to_view(&settings);
    view.updated_at = state
        .get_qa_settings_updated_at()
        .await
        .map_err(|error| error.to_string())?;
    Ok(view)
}

#[tauri::command]
pub async fn save_qa_settings(
    settings: QaSettingsView,
    state: tauri::State<'_, Database>,
) -> Result<QaSettingsView, String> {
    let payload = QaSettings {
        enabled: settings.enabled,
        provider: settings.provider,
        base_url: settings.base_url,
        api_key: settings.api_key,
        model: settings.model,
        temperature: settings.temperature,
        max_output_tokens: settings.max_output_tokens,
        context_chunk_limit: settings.context_chunk_limit,
        context_token_budget: settings.context_token_budget,
        min_evidence_count: settings.min_evidence_count,
        min_retrieval_score: settings.min_retrieval_score,
    };
    state
        .save_qa_settings(&payload)
        .await
        .map_err(|error| error.to_string())?;
    let settings = state
        .get_qa_settings()
        .await
        .map_err(|error| error.to_string())?;
    let mut view = qa_settings_to_view(&settings);
    view.updated_at = state
        .get_qa_settings_updated_at()
        .await
        .map_err(|error| error.to_string())?;
    Ok(view)
}

#[tauri::command]
pub async fn test_qa_connection(settings: QaSettingsView) -> Result<QaConnectionTestView, String> {
    if settings.base_url.trim().is_empty() {
        return Err("API Base URL 不能为空".to_string());
    }
    if settings.model.trim().is_empty() {
        return Err("模型名称不能为空".to_string());
    }

    let base_url = settings.base_url.clone();
    let api_key = settings.api_key.clone();
    let model = settings.model.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        ask_model(
            &base_url,
            &api_key,
            &model,
            "ping",
            "You are a connection test for DocMind. Reply with a short confirmation only.",
            0.0,
            16,
        )
    })
    .await
    .map_err(|error| error.to_string())?;

    match result {
        Ok(message) => Ok(QaConnectionTestView {
            ok: true,
            message: format!("连接成功: {}", message.trim()),
        }),
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub async fn list_qa_history(
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<QaHistoryView>, String> {
    state
        .list_qa_history(limit as i64)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_qa_history(
    id: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_qa_history(&id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_qa_session(
    title: String,
    state: tauri::State<'_, Database>,
) -> Result<QaSessionView, String> {
    state
        .create_qa_session(&title)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_qa_sessions(
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<QaSessionView>, String> {
    state
        .list_qa_sessions(limit as i64)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_qa_messages(
    session_id: String,
    limit: usize,
    state: tauri::State<'_, Database>,
) -> Result<Vec<QaMessageView>, String> {
    state
        .list_qa_messages(&session_id, limit as i64)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_qa_session(
    session_id: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_qa_session(&session_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_qa_session_title(
    session_id: String,
    title: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .update_qa_session_title(&session_id, &title)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn ask_question(
    app: tauri::AppHandle,
    question: String,
    scope_paths: Vec<String>,
    limit: usize,
    session_id: Option<String>,
    state: tauri::State<'_, Database>,
) -> Result<QaAskStartView, String> {
    let normalized_question = question.trim().to_string();
    if normalized_question.is_empty() {
        return Err("问题不能为空".to_string());
    }

    let settings = state.get_qa_settings().await.map_err(|error| error.to_string())?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let answer_id = uuid::Uuid::new_v4().to_string();
    let job_id = answer_id.clone();
    let normalized_session_id = session_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    let session_messages = if let Some(session_id) = normalized_session_id.as_deref() {
        state
            .list_qa_messages_recent(session_id, 6)
            .await
            .map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    let session_context = build_session_context(&session_messages);
    let session_terms = build_session_terms(&session_messages, &normalize_query(&normalized_question));
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    register_qa_job(job_id.clone(), abort_handle);
    let database = state.inner().clone();
    let cancel_meta = Arc::new(Mutex::new(None::<(Vec<QaSourceView>, QaRetrievalView)>));

    let emit_progress = |payload: QaAnswerProgressView| {
        let _ = app.emit("docmind:qa:answer-progress", payload);
    };

    if !settings.enabled || settings.base_url.trim().is_empty() || settings.model.trim().is_empty() {
        let answer = build_answer_view(
            answer_id,
            normalized_question.clone(),
            String::new(),
            "model_not_configured".to_string(),
            Vec::new(),
            crate::docmind::models::QaRetrievalView {
                search_mode: String::new(),
                candidate_count: 0,
                selected_count: 0,
                semantic_enabled: false,
                semantic_fallback: false,
                semantic_fallback_reason: String::new(),
            },
            settings.model,
            created_at,
            Some("问答模型未启用或未配置".to_string()),
        );
        emit_progress(build_progress_view(
            job_id.clone(),
            "model_not_configured".to_string(),
            normalized_question.clone(),
            String::new(),
            Vec::new(),
            answer.retrieval.clone(),
            answer.model.clone(),
            answer.error.clone(),
        ));
        let _ = state
            .record_qa_answer(&answer, normalized_session_id.as_deref())
            .await;
        clear_qa_job(&job_id);
        return Ok(QaAskStartView {
            job_id,
            status: answer,
        });
    }

    emit_progress(build_progress_view(
        job_id.clone(),
        "searching".to_string(),
        normalized_question.clone(),
        String::new(),
        Vec::new(),
        QaRetrievalView {
            search_mode: String::new(),
            candidate_count: 0,
            selected_count: 0,
            semantic_enabled: false,
            semantic_fallback: false,
            semantic_fallback_reason: String::new(),
        },
        settings.model.clone(),
        None,
    ));

    let question_for_task = normalized_question.clone();
    let scope_paths_for_task = scope_paths.clone();
    let settings_for_task = settings.clone();
    let database_for_task = database.clone();
    let session_id_for_task = normalized_session_id.clone();
    let session_terms_for_task = session_terms.clone();
    let session_context_for_task = session_context.clone();
    let app_for_task = app.clone();
    let job_id_for_task = job_id.clone();
    let answer_id_for_task = answer_id.clone();
    let created_at_for_task = created_at.clone();
    let model_for_task = settings.model.clone();
    let cancel_app = app.clone();
    let cancel_job_id = job_id.clone();
    let cancel_answer_id = answer_id.clone();
    let cancel_question = normalized_question.clone();
    let cancel_model = settings.model.clone();
    let cancel_created_at = created_at.clone();
    let cancel_database = database.clone();
    let cancel_session_id = normalized_session_id.clone();
    let cancel_meta_for_cancel = cancel_meta.clone();
    let clear_job_id = job_id.clone();
    let start_status = build_answer_view(
        answer_id.clone(),
        normalized_question.clone(),
        String::new(),
        "running".to_string(),
        Vec::new(),
        QaRetrievalView {
            search_mode: String::new(),
            candidate_count: 0,
            selected_count: 0,
            semantic_enabled: false,
            semantic_fallback: false,
            semantic_fallback_reason: String::new(),
        },
        settings.model.clone(),
        created_at.clone(),
        None,
    );

    tauri::async_runtime::spawn(async move {
        let task = Abortable::new(
            async move {
                let context = match build_qa_context(
                    &database_for_task,
                    &question_for_task,
                    &scope_paths_for_task,
                    &settings_for_task,
                    limit,
                    &session_terms_for_task,
                )
                .await
                {
                    Ok(context) => context,
                    Err(error) => {
                        let answer = build_answer_view(
                            answer_id_for_task,
                            question_for_task.clone(),
                            String::new(),
                            "failed".to_string(),
                            Vec::new(),
                            QaRetrievalView {
                                search_mode: String::new(),
                                candidate_count: 0,
                                selected_count: 0,
                                semantic_enabled: false,
                                semantic_fallback: false,
                                semantic_fallback_reason: String::new(),
                            },
                            model_for_task.clone(),
                            created_at_for_task.clone(),
                            Some(error.clone()),
                        );
                        let _ = app_for_task.emit(
                            "docmind:qa:answer-progress",
                            build_progress_view(
                                job_id_for_task.clone(),
                                "failed".to_string(),
                                question_for_task.clone(),
                                String::new(),
                                Vec::new(),
                                answer.retrieval.clone(),
                                answer.model.clone(),
                                answer.error.clone(),
                            ),
                        );
                        let _ = database_for_task
                            .record_qa_answer(&answer, session_id_for_task.as_deref())
                            .await;
                        return Ok::<(), String>(());
                    }
                };

                if context.sources.len() < settings_for_task.min_evidence_count {
                    let sources = context
                        .sources
                        .into_iter()
                        .map(|item| item.source)
                        .collect::<Vec<_>>();
                    let retrieval = context.retrieval;
                    let evidence_error = format!(
                        "无法从当前已索引文档中找到足够来源来回答这个问题；候选 {} 条，选中 {} 条，最少需要 {} 条",
                        retrieval.candidate_count,
                        retrieval.selected_count,
                        settings_for_task.min_evidence_count
                    );
                    let answer = build_answer_view(
                        answer_id_for_task,
                        question_for_task.clone(),
                        String::new(),
                        "insufficient_evidence".to_string(),
                        sources.clone(),
                        retrieval,
                        model_for_task.clone(),
                        created_at_for_task.clone(),
                        Some(evidence_error),
                    );
                    let _ = app_for_task.emit(
                        "docmind:qa:answer-progress",
                        build_progress_view(
                            job_id_for_task.clone(),
                            "insufficient_evidence".to_string(),
                            question_for_task.clone(),
                            String::new(),
                            sources,
                            answer.retrieval.clone(),
                            answer.model.clone(),
                            answer.error.clone(),
                        ),
                    );
                    let _ = database_for_task
                        .record_qa_answer(&answer, session_id_for_task.as_deref())
                        .await;
                    return Ok::<(), String>(());
                }

                let prompt = build_system_prompt(&context, Some(session_context_for_task.as_str()));
                let sources = context
                    .sources
                    .iter()
                    .map(|item| item.source.clone())
                    .collect::<Vec<_>>();
                let retrieval = context.retrieval.clone();
                if let Ok(mut guard) = cancel_meta.lock() {
                    *guard = Some((sources.clone(), retrieval.clone()));
                }
                let _ = app_for_task.emit(
                    "docmind:qa:answer-progress",
                    build_progress_view(
                        job_id_for_task.clone(),
                        "generating".to_string(),
                        question_for_task.clone(),
                        String::new(),
                        sources.clone(),
                        retrieval.clone(),
                        model_for_task.clone(),
                        None,
                    ),
                );

                let streamed = ask_model_stream(
                    &settings_for_task.base_url,
                    &settings_for_task.api_key,
                    &settings_for_task.model,
                    &question_for_task,
                    &prompt,
                    settings_for_task.temperature,
                    settings_for_task.max_output_tokens,
                    |partial| {
                        let _ = app_for_task.emit(
                            "docmind:qa:answer-progress",
                            build_progress_view(
                                job_id_for_task.clone(),
                                "streaming".to_string(),
                                question_for_task.clone(),
                                partial,
                                sources.clone(),
                                retrieval.clone(),
                                model_for_task.clone(),
                                None,
                            ),
                        );
                        Ok(())
                    },
                )
                .await;

                match streamed {
                    Ok(answer_text) => {
                        let answer = build_answer_view(
                            answer_id_for_task,
                            question_for_task.clone(),
                            answer_text.clone(),
                            "answered".to_string(),
                            sources.clone(),
                            retrieval.clone(),
                            model_for_task.clone(),
                            created_at_for_task.clone(),
                            None,
                        );
                        let _ = app_for_task.emit(
                            "docmind:qa:answer-progress",
                            build_progress_view(
                                job_id_for_task.clone(),
                                "answered".to_string(),
                                question_for_task.clone(),
                                answer_text,
                                sources,
                                retrieval,
                                model_for_task.clone(),
                                None,
                            ),
                        );
                        let _ = database_for_task
                            .record_qa_answer(&answer, session_id_for_task.as_deref())
                            .await;
                    }
                    Err(error) => {
                        let is_cancelled = error == "task has been cancelled" || error == "aborted";
                        let state = if is_cancelled { "cancelled" } else { "failed" };
                        let error_message = if is_cancelled {
                            "用户已取消问答".to_string()
                        } else {
                            error.clone()
                        };
                        let answer = build_answer_view(
                            answer_id_for_task,
                            question_for_task.clone(),
                            String::new(),
                            state.to_string(),
                            sources.clone(),
                            retrieval.clone(),
                            model_for_task.clone(),
                            created_at_for_task.clone(),
                            Some(error_message.clone()),
                        );
                        let _ = app_for_task.emit(
                            "docmind:qa:answer-progress",
                            build_progress_view(
                                job_id_for_task.clone(),
                                state.to_string(),
                                question_for_task,
                                String::new(),
                                sources,
                                retrieval,
                                model_for_task.clone(),
                                Some(error_message),
                            ),
                        );
                        let _ = database_for_task
                            .record_qa_answer(&answer, session_id_for_task.as_deref())
                            .await;
                    }
                }

                Ok::<(), String>(())
            },
            abort_registration,
        );

        match task.await {
            Ok(_) => {}
            Err(_) => {
                let (sources, retrieval) = cancel_meta_for_cancel
                    .lock()
                    .ok()
                    .and_then(|guard| guard.clone())
                    .unwrap_or_else(|| {
                        (
                            Vec::new(),
                            QaRetrievalView {
                                search_mode: String::new(),
                                candidate_count: 0,
                                selected_count: 0,
                                semantic_enabled: false,
                                semantic_fallback: false,
                                semantic_fallback_reason: String::new(),
                            },
                        )
                    });
                let error = "用户已取消问答".to_string();
                let answer = build_answer_view(
                    cancel_answer_id,
                    cancel_question.clone(),
                    String::new(),
                    "cancelled".to_string(),
                    sources.clone(),
                    retrieval.clone(),
                    cancel_model.clone(),
                    cancel_created_at.clone(),
                    Some(error.clone()),
                );
                let _ = cancel_app.emit(
                    "docmind:qa:answer-progress",
                    build_progress_view(
                        cancel_job_id.clone(),
                        "cancelled".to_string(),
                        cancel_question,
                        String::new(),
                        sources,
                        retrieval,
                        cancel_model.clone(),
                        Some(error),
                    ),
                );
                let _ = cancel_database
                    .record_qa_answer(&answer, cancel_session_id.as_deref())
                    .await;
            }
        }
        clear_qa_job(&clear_job_id);
    });

    Ok(QaAskStartView {
        job_id,
        status: start_status,
    })
}
