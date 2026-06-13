/**
 * @author MorningSun
 * @CreatedDate 2026/06/04
 * @Description Python 侧问答 sidecar 调用与 rag_answer_stream 协议适配。
 */
use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::seekmind::models::{QaRetrievalView, QaSourceView};
use crate::seekmind::sidecar::{resolve_timeout_ms, PythonSidecarRuntime};

#[derive(Debug, Clone)]
pub struct PythonQaClient {
    runtime: PythonSidecarRuntime,
    timeout: Duration,
}

#[derive(Debug, Serialize)]
pub struct RagSettingsRequest {
    pub provider: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_output_tokens: usize,
    pub context_chunk_limit: usize,
    pub context_token_budget: usize,
    pub min_evidence_count: usize,
    pub min_retrieval_score: f32,
}

#[derive(Debug, Serialize)]
pub struct RagRequest {
    pub request_id: String,
    pub command: String,
    pub db_path: String,
    pub question: String,
    pub session_id: Option<String>,
    pub scope_paths: Vec<String>,
    pub session_context: String,
    pub recent_questions: Vec<String>,
    pub settings: RagSettingsRequest,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RagProgressEvent {
    pub request_id: String,
    pub kind: String,
    pub event: String,
    pub stage: String,
    pub message: String,
    #[serde(default)]
    pub answer_delta: Option<String>,
    #[serde(default)]
    pub percent: i64,
    #[serde(default)]
    pub current: String,
    #[serde(default)]
    pub total: usize,
    #[serde(default)]
    pub processed: usize,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RagError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RagResponse {
    pub kind: String,
    pub request_id: String,
    pub ok: bool,
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub error: Option<RagError>,
    #[serde(default)]
    pub retrieval: Option<QaRetrievalView>,
    #[serde(default)]
    pub sources: Vec<QaSourceView>,
}

#[derive(Debug)]
pub enum QaClientError {
    NotConfigured,
    Timeout,
    SpawnFailed(String),
    Io(String),
    InvalidResponse(String),
}

impl Display for QaClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "python qa sidecar is not configured"),
            Self::Timeout => write!(f, "python qa sidecar timed out"),
            Self::SpawnFailed(error) => write!(f, "failed to spawn python qa sidecar: {error}"),
            Self::Io(error) => write!(f, "python qa sidecar io error: {error}"),
            Self::InvalidResponse(error) => {
                write!(f, "python qa sidecar returned invalid response: {error}")
            }
        }
    }
}

impl std::error::Error for QaClientError {}

impl PythonQaClient {
    pub fn from_env() -> Self {
        // 修复：问答 sidecar 入口统一复用 parser runtime 解析，同时修正历史的 SEEKMIND_parser 路径拼写错误。
        let runtime = PythonSidecarRuntime::from_env("parser/seekmind_parser/__main__.py");
        let timeout_ms =
            resolve_timeout_ms("SEEKMIND_QA_TIMEOUT_MS", Some("SEEKMIND_PARSER_TIMEOUT_MS"), 300_000);

        Self {
            runtime,
            timeout: Duration::from_millis(timeout_ms),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.runtime.is_configured()
    }

    pub fn ask_question_stream<F>(
        &self,
        request: &RagRequest,
        mut on_event: F,
    ) -> Result<RagResponse, QaClientError>
    where
        F: FnMut(RagProgressEvent),
    {
        if !self.is_configured() {
            return Err(QaClientError::NotConfigured);
        }

        let payload = serde_json::to_vec(request)
            .map_err(|error| QaClientError::Io(error.to_string()))?;
        let mut child = self
            .spawn_command()?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| QaClientError::SpawnFailed(error.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&payload)
                .map_err(|error| QaClientError::Io(error.to_string()))?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| QaClientError::SpawnFailed("missing stdout pipe".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| QaClientError::SpawnFailed("missing stderr pipe".to_string()))?;

        let (tx, rx) = mpsc::channel::<String>();
        let request_id = request.request_id.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                let _ = tx.send(line);
            }
        });

        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut buf = String::new();
            let mut sink = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => sink.push_str(&buf),
                }
            }
            if !sink.trim().is_empty() {
                eprintln!("[seekmind:qa] python stderr: {}", sink.trim());
            }
        });

        let stream_timeout = std::env::var("SEEKMIND_QA_STREAM_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_else(|| std::cmp::max(self.timeout, Duration::from_secs(300)));
        let mut last_activity = Instant::now();
        let mut response: Option<RagResponse> = None;

        loop {
            if Instant::now().duration_since(last_activity) >= stream_timeout {
                let _ = child.kill();
                return Err(QaClientError::Timeout);
            }

            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    last_activity = Instant::now();
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let event_value: Result<RagProgressEvent, _> = serde_json::from_str(trimmed);
                    if let Ok(event) = event_value {
                        if event.kind == "event"
                            && (event.request_id.is_empty() || event.request_id == request_id)
                        {
                            on_event(event);
                            continue;
                        }
                    }

                    let parsed: Result<RagResponse, _> = serde_json::from_str(trimmed);
                    match parsed {
                        Ok(parsed) => {
                            if parsed.request_id != request.request_id {
                                return Err(QaClientError::InvalidResponse(
                                    "request_id mismatch".to_string(),
                                ));
                            }
                            response = Some(parsed);
                        }
                        Err(error) => {
                            return Err(QaClientError::InvalidResponse(error.to_string()));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if response.is_some() {
                        if let Some(status) = child
                            .try_wait()
                            .map_err(|error| QaClientError::Io(error.to_string()))?
                        {
                            let _ = status;
                            break;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if response.is_some() {
                        break;
                    }
                    return Err(QaClientError::InvalidResponse(
                        "python qa stream closed unexpectedly".to_string(),
                    ));
                }
            }
        }

        response.ok_or_else(|| QaClientError::InvalidResponse("missing response".to_string()))
    }

    fn spawn_command(&self) -> Result<Command, QaClientError> {
        self.runtime
            .spawn_command()
            .ok_or(QaClientError::NotConfigured)
    }
}
