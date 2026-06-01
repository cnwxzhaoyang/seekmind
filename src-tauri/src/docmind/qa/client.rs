use futures_util::StreamExt;
use reqwest::{blocking::Client, Client as AsyncClient, Proxy};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct ChatCompletionsRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    max_tokens: usize,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChunk {
    choices: Vec<ChatStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamChoice {
    delta: ChatStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatStreamDelta {
    content: Option<String>,
}

pub fn ask_model(
    base_url: &str,
    api_key: &str,
    model: &str,
    question: &str,
    prompt: &str,
    temperature: f32,
    max_output_tokens: usize,
    proxy_url: Option<&str>,
) -> Result<String, String> {
    let client = build_blocking_client(proxy_url)?;

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let request = ChatCompletionsRequest {
        model,
        messages: vec![
            ChatMessage {
                role: "system",
                content: prompt,
            },
            ChatMessage {
                role: "user",
                content: question,
            },
        ],
        temperature,
        max_tokens: max_output_tokens,
        stream: false,
    };

    let mut builder = client.post(url).json(&request);
    if !api_key.trim().is_empty() {
        builder = builder.bearer_auth(api_key.trim());
    }

    let response = builder.send().map_err(|error| format_request_error("模型请求失败", error))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("模型请求失败: {status} {body}"));
    }

    let parsed: ChatCompletionsResponse = response
        .json()
        .map_err(|error| format!("模型响应解析失败: {error}"))?;
    let answer = parsed
        .choices
        .into_iter()
        .find_map(|choice| choice.message.content)
        .unwrap_or_default();

    let answer = answer.trim().to_string();
    if answer.is_empty() {
        return Err("模型返回为空".to_string());
    }

    Ok(answer)
}

fn format_request_error(prefix: &str, error: reqwest::Error) -> String {
    if error.is_timeout() {
        return format!("{prefix}: 模型服务响应超时");
    }

    if error.is_connect() {
        return format!("{prefix}: 无法连接模型服务");
    }

    format!("{prefix}: {error}")
}

pub async fn ask_model_stream<F>(
    base_url: &str,
    api_key: &str,
    model: &str,
    question: &str,
    prompt: &str,
    temperature: f32,
    max_output_tokens: usize,
    proxy_url: Option<&str>,
    mut on_delta: F,
) -> Result<String, String>
where
    F: FnMut(String) -> Result<(), String> + Send,
{
    let client = build_async_client(proxy_url)?;

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let request = ChatCompletionsRequest {
        model,
        messages: vec![
            ChatMessage {
                role: "system",
                content: prompt,
            },
            ChatMessage {
                role: "user",
                content: question,
            },
        ],
        temperature,
        max_tokens: max_output_tokens,
        stream: true,
    };

    let mut builder = client.post(url).json(&request);
    if !api_key.trim().is_empty() {
        builder = builder.bearer_auth(api_key.trim());
    }
    builder = builder.header("Accept", "text/event-stream");

    let response = builder
        .send()
        .await
        .map_err(|error| format_request_error("模型请求失败", error))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("模型请求失败: {status} {body}"));
    }

    let mut answer = String::new();

    let mut buffer = String::new();
    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|error| error.to_string())?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let mut line = buffer[..line_end].to_string();
            buffer.drain(..=line_end);
            line = line.trim_end_matches('\r').to_string();

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !trimmed.starts_with("data:") {
                continue;
            }

            let payload = trimmed.trim_start_matches("data:").trim();
            if payload == "[DONE]" {
                let answer = answer.trim().to_string();
                if answer.is_empty() {
                    return Err("模型返回为空".to_string());
                }
                return Ok(answer);
            }

            let chunk: ChatStreamChunk =
                serde_json::from_str(payload).map_err(|error| format!("模型流解析失败: {error}"))?;
            for choice in chunk.choices {
                if let Some(content) = choice.delta.content {
                    if !content.is_empty() {
                        answer.push_str(&content);
                        on_delta(answer.clone())?;
                    }
                }
                if choice.finish_reason.is_some() {
                    break;
                }
            }
        }
    }

    if !buffer.trim().is_empty() {
        let line = buffer.trim().trim_end_matches('\r');
        if line.starts_with("data:") {
            let payload = line.trim_start_matches("data:").trim();
            if payload != "[DONE]" {
                let chunk: ChatStreamChunk = serde_json::from_str(payload)
                    .map_err(|error| format!("模型流解析失败: {error}"))?;
                for choice in chunk.choices {
                    if let Some(content) = choice.delta.content {
                        if !content.is_empty() {
                            answer.push_str(&content);
                            on_delta(answer.clone())?;
                        }
                    }
                }
            }
        }
    }

    let answer = answer.trim().to_string();
    if answer.is_empty() {
        return Err("模型返回为空".to_string());
    }

    Ok(answer)
}

fn build_blocking_client(proxy_url: Option<&str>) -> Result<Client, String> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120));

    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        let proxy = Proxy::all(proxy_url).map_err(|error| format!("代理地址无效: {error}"))?;
        builder = builder.proxy(proxy);
    }

    builder.build().map_err(|error| error.to_string())
}

fn build_async_client(proxy_url: Option<&str>) -> Result<AsyncClient, String> {
    let mut builder = AsyncClient::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120));

    if let Some(proxy_url) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
        let proxy = Proxy::all(proxy_url).map_err(|error| format!("代理地址无效: {error}"))?;
        builder = builder.proxy(proxy);
    }

    builder.build().map_err(|error| error.to_string())
}
