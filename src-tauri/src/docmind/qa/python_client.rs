/**
 * @author MorningSun
 * @CreatedDate 2026/06/04
 * @Description Python 侧问答 sidecar 调用与 rag_answer_stream 协议适配。
 */
use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::docmind::models::{QaRetrievalView, QaSourceView};
use crate::docmind::sidecar::{configure_sidecar_command, resolve_bundled_sidecar};

#[derive(Debug, Clone)]
pub struct PythonQaClient {
    python_bin: String,
    script_path: PathBuf,
    bundled_sidecar: Option<PathBuf>,
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
        let python_bin =
            std::env::var("DOCMIND_PARSER_BIN").unwrap_or_else(|_| "python3".to_string());
        let script_path = std::env::var("DOCMIND_PARSER_SCRIPT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("parser/docmind_parser/__main__.py"));
        let bundled_sidecar = std::env::var("DOCMIND_PARSER_BIN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| {
                let candidate = PathBuf::from(value);
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            })
            .or_else(|| resolve_bundled_sidecar("docmind-parser").filter(|path| path.exists()));
        let timeout_ms = std::env::var("DOCMIND_QA_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| {
                std::env::var("DOCMIND_PARSER_TIMEOUT_MS")
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or(300_000);

        Self {
            python_bin,
            script_path,
            bundled_sidecar,
            timeout: Duration::from_millis(timeout_ms),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.bundled_sidecar.is_some() || self.resolve_script_path().exists()
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
                eprintln!("[docmind:qa] python stderr: {}", sink.trim());
            }
        });

        let stream_timeout = std::env::var("DOCMIND_QA_STREAM_TIMEOUT_MS")
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

    fn resolve_script_path(&self) -> PathBuf {
        if self.script_path.is_absolute() {
            return self.script_path.clone();
        }

        let candidates = [
            std::env::current_dir()
                .ok()
                .map(|cwd| cwd.join(&self.script_path)),
            std::env::current_dir()
                .ok()
                .map(|cwd| cwd.join("src-tauri").join(&self.script_path)),
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|parent| parent.join(&self.script_path))),
            Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join(&self.script_path),
            ),
        ];

        for candidate in candidates.into_iter().flatten() {
            if candidate.exists() {
                return candidate;
            }
        }

        self.script_path.clone()
    }

    fn spawn_command(&self) -> Result<Command, QaClientError> {
        if let Some(path) = &self.bundled_sidecar {
            let mut command = Command::new(path);
            configure_sidecar_command(&mut command);
            return Ok(command);
        }

        let script_path = self.resolve_script_path();
        if !script_path.exists() {
            return Err(QaClientError::NotConfigured);
        }

        let mut command = Command::new(&self.python_bin);
        command.arg(script_path);
        configure_sidecar_command(&mut command);
        Ok(command)
    }
}
