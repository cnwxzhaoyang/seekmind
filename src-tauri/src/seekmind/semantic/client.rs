/**
 * @author MorningSun
 * @CreatedDate 2026/06/12
 * @Description Python 语义 sidecar 客户端，负责 embedding 状态探测与向量生成请求。
 */
use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::seekmind::parser::types::ParserStreamEvent;
use crate::seekmind::sidecar::{configure_sidecar_command, resolve_bundled_sidecar};

#[derive(Debug, Clone)]
pub struct PythonSemanticClient {
    python_bin: String,
    script_path: PathBuf,
    bundled_sidecar: Option<PathBuf>,
    timeout: Duration,
}

#[derive(Debug, Serialize)]
struct SemanticRequest {
    request_id: String,
    command: String,
    path: String,
    options: SemanticOptions,
    texts: Vec<String>,
    model_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SemanticStatus {
    pub available: bool,
    pub provider: String,
    pub model_name: String,
    pub model_path: String,
    pub dimension: usize,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SemanticResponse {
    request_id: String,
    ok: bool,
    document: Option<serde_json::Value>,
    vectors: Option<Vec<Vec<f32>>>,
    embedding_status: Option<SemanticStatus>,
    error: Option<SemanticError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SemanticOptions {
    include_chunks: bool,
    max_chunk_chars: usize,
    max_chunks: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SemanticError {
    code: String,
    message: String,
    details: Option<String>,
}

#[derive(Debug)]
pub enum SemanticClientError {
    NotConfigured,
    Timeout,
    SpawnFailed(String),
    Io(String),
    InvalidResponse(String),
    SidecarFailed(String),
}

impl Display for SemanticClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "python semantic sidecar is not configured"),
            Self::Timeout => write!(f, "python semantic sidecar timed out"),
            Self::SpawnFailed(error) => {
                write!(f, "failed to spawn python semantic sidecar: {error}")
            }
            Self::Io(error) => write!(f, "python semantic sidecar io error: {error}"),
            Self::InvalidResponse(error) => write!(
                f,
                "python semantic sidecar returned invalid response: {error}"
            ),
            Self::SidecarFailed(error) => write!(f, "python semantic sidecar failed: {error}"),
        }
    }
}

impl std::error::Error for SemanticClientError {}

impl PythonSemanticClient {
    pub fn from_env() -> Self {
        let python_bin =
            std::env::var("SEEKMIND_PARSER_BIN").unwrap_or_else(|_| "python3".to_string());
        let script_path = std::env::var("SEEKMIND_PARSER_SCRIPT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("parser/seekmind_parser/__main__.py"));
        let bundled_sidecar = std::env::var("SEEKMIND_PARSER_BIN")
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
            .or_else(|| resolve_bundled_sidecar("seekmind-parser").filter(|path| path.exists()));
        let timeout_ms = std::env::var("SEEKMIND_SEMANTIC_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| {
                std::env::var("SEEKMIND_PARSER_TIMEOUT_MS")
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

    pub fn embedding_status(
        &self,
        model_name: Option<&str>,
    ) -> Result<SemanticStatus, SemanticClientError> {
        let response = self.execute("embedding_status", "", &[], model_name)?;
        response.embedding_status.ok_or_else(|| {
            SemanticClientError::InvalidResponse("missing embedding_status".to_string())
        })
    }

    pub fn embed_texts(
        &self,
        texts: &[String],
        model_name: Option<&str>,
    ) -> Result<Vec<Vec<f32>>, SemanticClientError> {
        self.embed_texts_stream(texts, model_name, |_| {})
    }

    pub fn embed_texts_stream<F>(
        &self,
        texts: &[String],
        model_name: Option<&str>,
        mut on_event: F,
    ) -> Result<Vec<Vec<f32>>, SemanticClientError>
    where
        F: FnMut(ParserStreamEvent),
    {
        if !self.is_configured() {
            return Err(SemanticClientError::NotConfigured);
        }

        let request = SemanticRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            command: "embed_texts_stream".to_string(),
            path: String::new(),
            options: SemanticOptions {
                include_chunks: true,
                max_chunk_chars: 800,
                max_chunks: None,
            },
            texts: texts.to_vec(),
            model_name: model_name.map(|value| value.to_string()),
        };

        let payload = serde_json::to_vec(&request)
            .map_err(|error| SemanticClientError::Io(error.to_string()))?;
        let mut child = self
            .spawn_command()?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| SemanticClientError::SpawnFailed(error.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&payload)
                .map_err(|error| SemanticClientError::Io(error.to_string()))?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SemanticClientError::SpawnFailed("missing stdout pipe".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SemanticClientError::SpawnFailed("missing stderr pipe".to_string()))?;

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
                eprintln!("[seekmind:semantic] python stderr: {}", sink.trim());
            }
        });

        let stream_timeout = std::env::var("SEEKMIND_SEMANTIC_STREAM_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_else(|| std::cmp::max(self.timeout, Duration::from_secs(300)));
        let mut last_activity = Instant::now();
        let mut response: Option<SemanticResponse> = None;

        loop {
            if Instant::now().duration_since(last_activity) >= stream_timeout {
                let _ = child.kill();
                return Err(SemanticClientError::Timeout);
            }

            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    last_activity = Instant::now();
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let event_value: Result<ParserStreamEvent, _> = serde_json::from_str(trimmed);
                    if let Ok(event) = event_value {
                        if event.kind == "event"
                            && (event.request_id.is_empty() || event.request_id == request_id)
                        {
                            on_event(event);
                            continue;
                        }
                    }

                    let parsed: Result<SemanticResponse, _> = serde_json::from_str(trimmed);
                    match parsed {
                        Ok(parsed) => {
                            if parsed.request_id != request.request_id {
                                return Err(SemanticClientError::InvalidResponse(
                                    "request_id mismatch".to_string(),
                                ));
                            }
                            response = Some(parsed);
                        }
                        Err(error) => {
                            return Err(SemanticClientError::InvalidResponse(error.to_string()));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if response.is_some() {
                        if let Some(status) = child
                            .try_wait()
                            .map_err(|error| SemanticClientError::Io(error.to_string()))?
                        {
                            if status.success() {
                                break;
                            }
                            return Err(SemanticClientError::SpawnFailed(format!(
                                "python semantic sidecar exited with status {status}"
                            )));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if response.is_some() {
                        break;
                    }
                    return Err(SemanticClientError::InvalidResponse(
                        "python semantic stream closed unexpectedly".to_string(),
                    ));
                }
            }
        }

        let response = response
            .ok_or_else(|| SemanticClientError::InvalidResponse("missing response".to_string()))?;

        if response.ok {
            response
                .vectors
                .ok_or_else(|| SemanticClientError::InvalidResponse("missing vectors".to_string()))
        } else {
            Err(SemanticClientError::SidecarFailed(
                response
                    .error
                    .map(|error| format!("{} ({})", error.message, error.code))
                    .unwrap_or_else(|| "embedding failed".to_string()),
            ))
        }
    }

    fn execute(
        &self,
        command: &str,
        path: &str,
        texts: &[String],
        model_name: Option<&str>,
    ) -> Result<SemanticResponse, SemanticClientError> {
        if !self.is_configured() {
            return Err(SemanticClientError::NotConfigured);
        }

        let request = SemanticRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            command: command.to_string(),
            path: path.to_string(),
            options: SemanticOptions {
                include_chunks: true,
                max_chunk_chars: 800,
                max_chunks: None,
            },
            texts: texts.to_vec(),
            model_name: model_name.map(|value| value.to_string()),
        };

        let payload = serde_json::to_vec(&request)
            .map_err(|error| SemanticClientError::Io(error.to_string()))?;
        let mut child = self
            .spawn_command()?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| SemanticClientError::SpawnFailed(error.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&payload)
                .map_err(|error| SemanticClientError::Io(error.to_string()))?;
        }

        let deadline = Instant::now() + self.timeout_for_command(command);
        loop {
            match child
                .try_wait()
                .map_err(|error| SemanticClientError::Io(error.to_string()))?
            {
                Some(_) => break,
                None => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        eprintln!(
                            "[SeekMind] semantic sidecar timeout command={} timeout_ms={}",
                            command,
                            self.timeout_for_command(command).as_millis()
                        );
                        return Err(SemanticClientError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }

        let output = child
            .wait_with_output()
            .map_err(|error| SemanticClientError::Io(error.to_string()))?;
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        let response: Result<SemanticResponse, _> = serde_json::from_slice(&output.stdout);
        match response {
            Ok(response) => {
                if response.request_id != request.request_id {
                    return Err(SemanticClientError::InvalidResponse(
                        "request_id mismatch".to_string(),
                    ));
                }
                Ok(response)
            }
            Err(error) => {
                if output.status.success() {
                    Err(SemanticClientError::InvalidResponse(error.to_string()))
                } else {
                    Err(SemanticClientError::SpawnFailed(if stderr.is_empty() {
                        format!("python semantic sidecar exited with non-zero status: {error}")
                    } else {
                        stderr
                    }))
                }
            }
        }
    }

    fn timeout_for_command(&self, command: &str) -> Duration {
        if command == "embedding_status" {
            return std::env::var("SEEKMIND_SEMANTIC_STATUS_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_millis)
                .unwrap_or_else(|| Duration::from_millis(15_000));
        }

        self.timeout
    }

    pub fn resolve_script_path(&self) -> PathBuf {
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

    fn spawn_command(&self) -> Result<Command, SemanticClientError> {
        if let Some(path) = &self.bundled_sidecar {
            let mut command = Command::new(path);
            configure_sidecar_command(&mut command);
            return Ok(command);
        }

        let script_path = self.resolve_script_path();
        if !script_path.exists() {
            return Err(SemanticClientError::NotConfigured);
        }

        let mut command = Command::new(&self.python_bin);
        command.arg(script_path);
        configure_sidecar_command(&mut command);
        Ok(command)
    }
}
