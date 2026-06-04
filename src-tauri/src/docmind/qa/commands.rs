/**
 * @author MorningSun
 * @CreatedDate 2026/06/04
 * @Description 问答命令入口，当前已切换为 Python sidecar 优先路由。
 */
use crate::docmind::models::{
    NetworkProxySettingsView, QaAnswerProgressView, QaAnswerView, QaAskStartView,
    QaConnectionTestView, QaHistoryView, QaMessageView, QaModelProfileUpsertView,
    QaModelProfileView, QaRetrievalView, QaSessionView, QaSettingsView, QaSourceView,
};
use crate::docmind::sidecar::apply_network_proxy_environment;
use crate::docmind::storage::db::Database;
use crate::docmind::storage::types::{NetworkProxySettings, QaSettings};

use super::cancel::{cancel as cancel_qa_job, clear as clear_qa_job, register as register_qa_job};
use super::python_client::{PythonQaClient, RagRequest, RagSettingsRequest};
use futures_util::future::{AbortHandle, Abortable};
use reqwest::Proxy;
use std::{
    fs,
    sync::{Arc, Mutex},
};
use tauri::Emitter;
use crate::docmind::storage::db::sqlite_database_path;

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

fn network_proxy_settings_to_view(
    settings: &NetworkProxySettings,
    updated_at: String,
) -> NetworkProxySettingsView {
    NetworkProxySettingsView {
        enabled: settings.enabled,
        proxy_url: settings.proxy_url.clone(),
        updated_at,
    }
}

fn truncate_prompt_text(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn build_session_context(messages: &[QaMessageView]) -> String {
    let mut lines = Vec::new();
    let recent_questions = messages
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(|message| truncate_prompt_text(message.question.trim(), 80))
        .filter(|question| !question.is_empty())
        .collect::<Vec<_>>();
    if !recent_questions.is_empty() {
        lines.push(format!("最近用户问题: {}", recent_questions.join(" -> ")));
    }

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
    warning: Option<String>,
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
        warning,
    }
}

fn build_progress_view(
    job_id: String,
    state: String,
    question: String,
    answer: String,
    answer_delta: String,
    sources: Vec<crate::docmind::models::QaSourceView>,
    retrieval: crate::docmind::models::QaRetrievalView,
    model: String,
    error: Option<String>,
    warning: Option<String>,
) -> QaAnswerProgressView {
    QaAnswerProgressView {
        job_id,
        state,
        question,
        answer,
        answer_delta,
        sources,
        retrieval,
        model,
        error,
        warning,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

#[tauri::command]
pub async fn cancel_qa_question(job_id: String) -> Result<(), String> {
    let _ = cancel_qa_job(job_id.trim());
    Ok(())
}

#[tauri::command]
pub async fn get_qa_settings(state: tauri::State<'_, Database>) -> Result<QaSettingsView, String> {
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
pub async fn save_qa_settings(
    settings: QaSettingsView,
    state: tauri::State<'_, Database>,
) -> Result<QaSettingsView, String> {
    let payload = QaSettings {
        // 修复：问答连接不再暴露手动启用开关，保存时统一写入启用态，避免默认连接和可用状态分裂。
        enabled: true,
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
pub async fn get_network_proxy_settings(
    state: tauri::State<'_, Database>,
) -> Result<NetworkProxySettingsView, String> {
    let settings = state
        .get_network_proxy_settings()
        .await
        .map_err(|error| error.to_string())?;
    let updated_at = state
        .get_network_proxy_settings_updated_at()
        .await
        .map_err(|error| error.to_string())?;
    Ok(network_proxy_settings_to_view(&settings, updated_at))
}

#[tauri::command]
pub async fn save_network_proxy_settings(
    settings: NetworkProxySettingsView,
    state: tauri::State<'_, Database>,
) -> Result<NetworkProxySettingsView, String> {
    let proxy_url = settings.proxy_url.trim().to_string();
    if settings.enabled && proxy_url.is_empty() {
        return Err("代理地址不能为空".to_string());
    }
    if !proxy_url.is_empty() {
        Proxy::all(proxy_url.as_str()).map_err(|error| format!("代理地址无效: {error}"))?;
    }

    let payload = NetworkProxySettings {
        enabled: settings.enabled,
        proxy_url,
    };
    state
        .save_network_proxy_settings(&payload)
        .await
        .map_err(|error| error.to_string())?;
    apply_network_proxy_environment(Some(&payload));
    let settings = state
        .get_network_proxy_settings()
        .await
        .map_err(|error| error.to_string())?;
    let updated_at = state
        .get_network_proxy_settings_updated_at()
        .await
        .map_err(|error| error.to_string())?;
    Ok(network_proxy_settings_to_view(&settings, updated_at))
}

#[tauri::command]
pub async fn list_qa_model_profiles(
    state: tauri::State<'_, Database>,
) -> Result<Vec<QaModelProfileView>, String> {
    state
        .list_qa_model_profiles()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn save_qa_model_profile(
    profile: QaModelProfileUpsertView,
    state: tauri::State<'_, Database>,
) -> Result<QaModelProfileView, String> {
    state
        .save_qa_model_profile(&profile)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_qa_model_profile(
    profile_id: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_qa_model_profile(profile_id.trim())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_default_qa_model_profile(
    profile_id: String,
    state: tauri::State<'_, Database>,
) -> Result<QaModelProfileView, String> {
    state
        .set_default_qa_model_profile(profile_id.trim())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn test_qa_connection(
    settings: QaSettingsView,
    state: tauri::State<'_, Database>,
) -> Result<QaConnectionTestView, String> {
    if settings.base_url.trim().is_empty() {
        return Err("API Base URL 不能为空".to_string());
    }
    if settings.model.trim().is_empty() {
        return Err("模型名称不能为空".to_string());
    }

    let base_url = settings.base_url.clone();
    let api_key = settings.api_key.clone();
    let model = settings.model.clone();
    let provider = settings.provider.clone();
    let proxy_settings = state
        .get_network_proxy_settings()
        .await
        .map_err(|error| error.to_string())?;
    let proxy_url = if proxy_settings.enabled && !proxy_settings.proxy_url.trim().is_empty() {
        Some(proxy_settings.proxy_url)
    } else {
        None
    };

    let result = tauri::async_runtime::spawn_blocking(move || {
        // 这里只做最薄的健康检查，不再保留完整问答实现。
        eprintln!(
            "[DocMind] testing qa model connection provider={} model={} base_url={}",
            provider, model, base_url
        );
        let mut builder = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(8))
            .timeout(std::time::Duration::from_secs(120));
        if let Some(proxy_url) = proxy_url.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            let proxy = Proxy::all(proxy_url).map_err(|error| format!("代理地址无效: {error}"))?;
            builder = builder.proxy(proxy);
        }
        let client = builder.build().map_err(|error| error.to_string())?;
        let is_ollama = provider.to_lowercase().contains("ollama")
            || base_url.contains("11434")
            || base_url.contains("/api/");
        let prompt = "You are a connection test for DocMind. Reply with a short confirmation only.";
        let user_content = "ping";

        let test_openai_compatible = |client: &reqwest::blocking::Client| -> Result<String, String> {
            let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
            let payload = serde_json::json!({
                "model": model,
                "messages": [
                    {
                        "role": "system",
                        "content": prompt
                    },
                    {
                        "role": "user",
                        "content": user_content
                    }
                ],
                "temperature": 0.0,
                "max_tokens": 16,
                "stream": false
            });
            let mut request = client.post(url).json(&payload);
            if !api_key.trim().is_empty() {
                request = request.bearer_auth(api_key.trim());
            }
            let response = request
                .send()
                .map_err(|error| format!("模型请求失败: {error}"))?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                return Err(format!("模型请求失败: {status} {body}"));
            }
            let parsed: serde_json::Value = response
                .json()
                .map_err(|error| format!("模型响应解析失败: {error}"))?;
            let message = parsed["choices"]
                .as_array()
                .and_then(|choices| choices.first())
                .and_then(|choice| {
                    choice["message"]["content"]
                        .as_str()
                        .or_else(|| choice["message"]["reasoning_content"].as_str())
                        .or_else(|| choice["message"]["thinking"].as_str())
                })
                .unwrap_or_default()
                .trim()
                .to_string();
            Ok(message)
        };

        let test_ollama_native = |client: &reqwest::blocking::Client| -> Result<String, String> {
            let mut native_base = base_url.trim_end_matches('/').to_string();
            if native_base.ends_with("/v1") {
                native_base.truncate(native_base.len() - 3);
            } else if native_base.ends_with("/openai") {
                native_base.truncate(native_base.len() - 7);
            }
            let url = format!("{}/api/chat", native_base.trim_end_matches('/'));
            let payload = serde_json::json!({
                "model": model,
                "messages": [
                    {
                        "role": "system",
                        "content": prompt
                    },
                    {
                        "role": "user",
                        "content": user_content
                    }
                ],
                "stream": false,
                "think": false,
                "options": {
                    "temperature": 0.0,
                    "num_predict": 16
                }
            });
            let mut request = client.post(url).json(&payload);
            if !api_key.trim().is_empty() {
                request = request.bearer_auth(api_key.trim());
            }
            let response = request
                .send()
                .map_err(|error| format!("模型请求失败: {error}"))?;
            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                return Err(format!("模型请求失败: {status} {body}"));
            }
            let parsed: serde_json::Value = response
                .json()
                .map_err(|error| format!("模型响应解析失败: {error}"))?;
            let message = parsed["message"]["content"]
                .as_str()
                .or_else(|| parsed["message"]["thinking"].as_str())
                .or_else(|| parsed["response"].as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            Ok(message)
        };

        let mut message = test_openai_compatible(&client)?;
        if message.is_empty() && is_ollama {
            // 修复：Ollama 的 OpenAI-compatible 端点在思考型模型上可能返回空 content，
            // 这里回退到原生 /api/chat，以避免把可用模型误判为“模型返回为空”。
            eprintln!(
                "[DocMind] qa connection empty on openai-compatible endpoint, retrying ollama native api"
            );
            message = test_ollama_native(&client)?;
        }
        if message.is_empty() {
            return Err("模型返回为空".to_string());
        }
        Ok(message)
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
pub async fn export_qa_session_markdown(path: String, markdown: String) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("导出路径不能为空".to_string());
    }

    fs::write(path, markdown).map_err(|error| format!("导出 Markdown 失败: {error}"))?;
    Ok(path.to_string())
}

#[tauri::command]
pub async fn ask_question(
    app: tauri::AppHandle,
    question: String,
    scope_paths: Vec<String>,
    limit: usize,
    session_id: Option<String>,
    recent_questions: Option<Vec<String>>,
    profile_id: Option<String>,
    state: tauri::State<'_, Database>,
) -> Result<QaAskStartView, String> {
    let normalized_question = question.trim().to_string();
    if normalized_question.is_empty() {
        return Err("问题不能为空".to_string());
    }

    // 迁移期保留问题重写，但 RAG 核心已经下沉到 Python sidecar，Rust 侧不再维护旧的构建/生成链路。
    let _ = limit; // Python 侧根据 SQLite 自主召回，Rust 端不再做 chunk 限制搬运。

    let settings = state
        .get_qa_settings()
        .await
        .map_err(|error| error.to_string())?;
    let proxy_settings = state
        .get_network_proxy_settings()
        .await
        .map_err(|error| error.to_string())?;
    let selected_profile = match profile_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
    {
        Some(profile_id) => {
            let profile = state
                .get_qa_model_profile(&profile_id)
                .await
                .map_err(|error| error.to_string())?;
            if profile.is_none() {
                return Err(format!("找不到模型连接: {profile_id}"));
            }
            profile
        }
        None => None,
    };
    let active_provider = selected_profile
        .as_ref()
        .map(|item| item.provider.clone())
        .unwrap_or_else(|| settings.provider.clone());
    let active_base_url = selected_profile
        .as_ref()
        .map(|item| item.base_url.clone())
        .unwrap_or_else(|| settings.base_url.clone());
    let _active_api_key = selected_profile
        .as_ref()
        .map(|item| item.api_key.clone())
        .unwrap_or_else(|| settings.api_key.clone());
    let active_model = selected_profile
        .as_ref()
        .map(|item| item.model.clone())
        .unwrap_or_else(|| settings.model.clone());
    let _active_proxy_url = if proxy_settings.enabled && !proxy_settings.proxy_url.trim().is_empty()
    {
        Some(proxy_settings.proxy_url.clone())
    } else {
        None
    };
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
    let recent_questions = recent_questions.unwrap_or_default();
    let session_context = build_session_context(&session_messages);

    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    register_qa_job(job_id.clone(), abort_handle);
    let database = state.inner().clone();
    let cancel_meta = Arc::new(Mutex::new(None::<(Vec<QaSourceView>, QaRetrievalView)>));

    let emit_progress = |payload: QaAnswerProgressView| {
        let _ = app.emit("docmind:qa:answer-progress", payload);
    };

    let python_client = PythonQaClient::from_env();
    if active_provider.trim().is_empty()
        || active_base_url.trim().is_empty()
        || active_model.trim().is_empty()
    {
        let answer = build_answer_view(
            answer_id,
            normalized_question.clone(),
            String::new(),
            "model_not_configured".to_string(),
            Vec::new(),
            QaRetrievalView {
                search_mode: String::new(),
                candidate_count: 0,
                selected_count: 0,
                semantic_enabled: false,
                semantic_fallback: false,
                semantic_fallback_reason: String::new(),
            },
            active_model.clone(),
            created_at,
            Some("问答模型未启用或未配置".to_string()),
            None,
        );
        emit_progress(build_progress_view(
            job_id.clone(),
            "model_not_configured".to_string(),
            normalized_question.clone(),
            String::new(),
            String::new(),
            Vec::new(),
            answer.retrieval.clone(),
            answer.model.clone(),
            answer.error.clone(),
            answer.warning.clone(),
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

    if !python_client.is_configured() {
        // 迁移期明确禁用 Rust 旧链路，避免把 sidecar 缺失误导成可回退路径。
        eprintln!(
            "[DocMind] python qa sidecar unavailable; Rust fallback qa path has been removed"
        );
        let answer = build_answer_view(
            answer_id,
            normalized_question.clone(),
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
            active_model.clone(),
            created_at,
            Some("问答 sidecar 未配置，Rust 旧问答链路已移除".to_string()),
            None,
        );
        emit_progress(build_progress_view(
            job_id.clone(),
            "failed".to_string(),
            normalized_question.clone(),
            String::new(),
            String::new(),
            Vec::new(),
            answer.retrieval.clone(),
            answer.model.clone(),
            answer.error.clone(),
            answer.warning.clone(),
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
        active_model.clone(),
        None,
        None,
    ));

    let question_for_task = normalized_question.clone();
    let session_context_for_task = session_context.clone();
    let scope_paths_for_task = scope_paths.clone();
    let recent_questions_for_task = recent_questions.clone();
    let settings_for_task = settings.clone();
    let session_id_for_task = normalized_session_id.clone();
    let app_for_task = app.clone();
    let job_id_for_task = job_id.clone();
    let answer_id_for_task = answer_id.clone();
    let created_at_for_task = created_at.clone();
    let model_for_task = active_model.clone();
    let database_for_task = database.clone();
    let python_client_for_task = python_client.clone();
    let cancel_app = app.clone();
    let cancel_job_id = job_id.clone();
    let cancel_answer_id = answer_id.clone();
    let cancel_question = normalized_question.clone();
    let cancel_model = active_model.clone();
    let cancel_created_at = created_at.clone();
    let cancel_database = database.clone();
    let cancel_session_id = normalized_session_id.clone();
    let cancel_meta_for_cancel = cancel_meta.clone();
    let clear_job_id = job_id.clone();
    let database_path = sqlite_database_path().to_string_lossy().to_string();
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
        active_model.clone(),
        created_at.clone(),
        None,
        None,
    );

    tauri::async_runtime::spawn(async move {
        let task = Abortable::new(
            async move {
                let request = RagRequest {
                    request_id: job_id_for_task.clone(),
                    command: "rag_answer_stream".to_string(),
                    db_path: database_path.clone(),
                    question: question_for_task.clone(),
                    session_id: session_id_for_task.clone(),
                    scope_paths: scope_paths_for_task.clone(),
                    session_context: session_context_for_task.clone(),
                    recent_questions: recent_questions_for_task.clone(),
                    settings: RagSettingsRequest {
                        provider: settings_for_task.provider.clone(),
                        base_url: settings_for_task.base_url.clone(),
                        api_key: settings_for_task.api_key.clone(),
                        model: settings_for_task.model.clone(),
                        temperature: settings_for_task.temperature,
                        max_output_tokens: settings_for_task.max_output_tokens,
                        context_chunk_limit: settings_for_task.context_chunk_limit,
                        context_token_budget: settings_for_task.context_token_budget,
                        min_evidence_count: settings_for_task.min_evidence_count,
                        min_retrieval_score: settings_for_task.min_retrieval_score,
                    },
                };

                eprintln!(
                    "[DocMind] routing qa request {} to python sidecar, scope_paths={}, session_context_bytes={}",
                    job_id_for_task,
                    request.scope_paths.len(),
                    request.session_context.len()
                );

                // 修复：Python 侧会逐步吐出 answer_delta，这里必须累计成当前正文后再转发给前端。
                // 只裁掉最前面的空白，避免模型流式首包带来的空行把 Markdown 预览撑出大段空白。
                let mut streamed_answer = String::new();
                let response = match python_client_for_task.ask_question_stream(&request, |event| {
                    if let Some(delta) = event.answer_delta.as_deref() {
                        if streamed_answer.is_empty() {
                            let trimmed = delta.trim_start();
                            if !trimmed.is_empty() {
                                streamed_answer.push_str(trimmed);
                            }
                        } else {
                            streamed_answer.push_str(delta);
                        }
                    }
                    let stage = match event.stage.as_str() {
                        "bootstrap" | "retrieve" => "searching",
                        // 修复：Python RAG Graph 新增了更细的阶段节点，这里需要把它们映射到前端已有的状态语义。
                        "rank" | "pack_evidence" => "searching",
                        "prompt" | "generate" => "generating",
                        "answer_delta" => "streaming",
                        "verify" => "verifying",
                        "judge" => "verifying",
                        "repair" | "finalize" => "verifying",
                        // 修复：Python 的 finish 只是最终 response 前的阶段事件，不携带正文；终态 answered 只能由最终 response 发出。
                        "finish" => "verifying",
                        "failed" => "failed",
                        _ => "running",
                    };
                    let _ = app_for_task.emit(
                        "docmind:qa:answer-progress",
                        build_progress_view(
                            job_id_for_task.clone(),
                            stage.to_string(),
                            question_for_task.clone(),
                            streamed_answer.clone(),
                            event.answer_delta.clone().unwrap_or_default(),
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
                            None,
                            event.warning.clone(),
                        ),
                    );
                }) {
                    Ok(response) => response,
                    Err(error) => {
                        let is_timeout = matches!(
                            error,
                            super::python_client::QaClientError::Timeout
                        );
                        let state = if is_timeout { "cancelled" } else { "failed" };
                        let error_message = if is_timeout {
                            "Python 问答 sidecar 超时".to_string()
                        } else {
                            error.to_string()
                        };
                        eprintln!(
                            "[DocMind] qa request {} failed in python sidecar: {}",
                            job_id_for_task, error_message
                        );
                        let answer = build_answer_view(
                            answer_id_for_task.clone(),
                            question_for_task.clone(),
                            String::new(),
                            state.to_string(),
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
                            Some(error_message.clone()),
                            None,
                        );
                        let _ = app_for_task.emit(
                            "docmind:qa:answer-progress",
                        build_progress_view(
                            job_id_for_task.clone(),
                            state.to_string(),
                            question_for_task.clone(),
                            String::new(),
                            String::new(),
                            Vec::new(),
                            answer.retrieval.clone(),
                            answer.model.clone(),
                                answer.error.clone(),
                                answer.warning.clone(),
                            ),
                        );
                        let _ = database_for_task
                            .record_qa_answer(&answer, session_id_for_task.as_deref())
                            .await;
                        return Ok::<(), String>(());
                    }
                };

                let retrieval = response.retrieval.clone().unwrap_or(QaRetrievalView {
                    search_mode: String::new(),
                    candidate_count: 0,
                    selected_count: 0,
                    semantic_enabled: false,
                    semantic_fallback: false,
                    semantic_fallback_reason: String::new(),
                });
                let sources = response.sources.clone();
                let state = if response.state.trim().is_empty() {
                    if response.ok {
                        "answered".to_string()
                    } else {
                        "failed".to_string()
                    }
                } else {
                    response.state.clone()
                };
                let error_message = response.error.as_ref().map(|error| {
                    if error.message.trim().is_empty() {
                        error.code.clone()
                    } else {
                        error.message.clone()
                    }
                });
                let warning = response.warning.clone();
                if let Ok(mut guard) = cancel_meta.lock() {
                    *guard = Some((sources.clone(), retrieval.clone()));
                }
                if let Some(ref warning_message) = warning {
                    eprintln!(
                        "[DocMind] qa request {} warning from python sidecar: {}",
                        job_id_for_task, warning_message
                    );
                }
                let answer = build_answer_view(
                    answer_id_for_task.clone(),
                    question_for_task.clone(),
                    response.answer.clone(),
                    state.clone(),
                    sources.clone(),
                    retrieval.clone(),
                    model_for_task.clone(),
                    created_at_for_task.clone(),
                    error_message.clone(),
                    warning.clone(),
                );
                let _ = app_for_task.emit(
                    "docmind:qa:answer-progress",
                    build_progress_view(
                        job_id_for_task.clone(),
                        state.clone(),
                        question_for_task.clone(),
                        response.answer.clone(),
                        String::new(),
                        sources.clone(),
                        retrieval.clone(),
                        model_for_task.clone(),
                        error_message.clone(),
                        warning.clone(),
                    ),
                );
                let _ = database_for_task
                    .record_qa_answer(&answer, session_id_for_task.as_deref())
                    .await;

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
                    None,
                );
                let _ = cancel_app.emit(
                    "docmind:qa:answer-progress",
                    build_progress_view(
                        cancel_job_id.clone(),
                        "cancelled".to_string(),
                        cancel_question,
                        String::new(),
                        String::new(),
                        sources,
                        retrieval,
                        cancel_model.clone(),
                        Some(error),
                        None,
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
