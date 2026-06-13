use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::seekmind::runtime_paths::office_converter_candidates;
use crate::seekmind::sidecar::{resolve_timeout_ms, PythonSidecarRuntime};

use super::types::{
    ParsedDocument, ParserError, ParserOptions, ParserRequest, ParserResponse, ParserStreamEvent,
};

#[derive(Debug, Clone)]
pub struct PythonParserClient {
    runtime: PythonSidecarRuntime,
    timeout: Duration,
}

#[derive(Debug)]
pub enum ParserClientError {
    NotConfigured,
    Timeout,
    SpawnFailed(String),
    Io(String),
    InvalidResponse(String),
    ParserFailed(ParserError),
}

impl Display for ParserClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "python parser is not configured"),
            Self::Timeout => write!(f, "python parser timed out"),
            Self::SpawnFailed(error) => write!(f, "failed to spawn python parser: {error}"),
            Self::Io(error) => write!(f, "python parser io error: {error}"),
            Self::InvalidResponse(error) => {
                write!(f, "python parser returned invalid response: {error}")
            }
            Self::ParserFailed(error) => write!(
                f,
                "python parser failed: {} ({})",
                error.message, error.code
            ),
        }
    }
}

impl std::error::Error for ParserClientError {}

impl PythonParserClient {
    pub fn from_env() -> Self {
        // 修复：parser / qa / semantic sidecar 的运行时入口必须共用同一套平台解析，避免 Windows 下 python 可执行名和 script 路径继续分叉。
        let runtime = PythonSidecarRuntime::from_env("parser/seekmind_parser/__main__.py");
        let timeout_ms = resolve_timeout_ms("SEEKMIND_PARSER_TIMEOUT_MS", None, 30_000);

        Self {
            runtime,
            timeout: Duration::from_millis(timeout_ms),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.runtime.is_configured()
    }

    pub fn parse_document(&self, path: &Path) -> Result<ParsedDocument, ParserClientError> {
        self.parse_document_stream(path, |_| {})
    }

    pub fn parse_document_stream<F>(
        &self,
        path: &Path,
        mut on_event: F,
    ) -> Result<ParsedDocument, ParserClientError>
    where
        F: FnMut(ParserStreamEvent),
    {
        if !self.is_configured() {
            return Err(ParserClientError::NotConfigured);
        }

        let request = ParserRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            command: "parse_document_stream".to_string(),
            path: path.to_string_lossy().to_string(),
            options: ParserOptions::default(),
        };

        let payload = serde_json::to_vec(&request)
            .map_err(|error| ParserClientError::Io(error.to_string()))?;

        let mut child = self
            .spawn_command()?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ParserClientError::SpawnFailed(error.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&payload)
                .map_err(|error| ParserClientError::Io(error.to_string()))?;
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ParserClientError::SpawnFailed("missing stdout pipe".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ParserClientError::SpawnFailed("missing stderr pipe".to_string()))?;

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
                    Ok(_) => {
                        sink.push_str(&buf);
                    }
                }
            }
            if !sink.trim().is_empty() {
                eprintln!("[seekmind:parser] python stderr: {}", sink.trim());
            }
        });

        let stream_timeout = std::env::var("SEEKMIND_PARSER_STREAM_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_else(|| std::cmp::max(self.timeout, Duration::from_secs(300)));
        let mut last_activity = Instant::now();
        let mut response: Option<ParserResponse> = None;

        loop {
            if Instant::now().duration_since(last_activity) >= stream_timeout {
                let _ = child.kill();
                return Err(ParserClientError::Timeout);
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
                        if event.kind == "event" {
                            if event.request_id.is_empty() || event.request_id == request_id {
                                on_event(event);
                            }
                            continue;
                        }
                    }

                    let parsed: Result<ParserResponse, _> = serde_json::from_str(trimmed);
                    match parsed {
                        Ok(parsed) => {
                            if parsed.request_id != request_id {
                                return Err(ParserClientError::InvalidResponse(
                                    "request_id mismatch".to_string(),
                                ));
                            }
                            response = Some(parsed);
                        }
                        Err(error) => {
                            return Err(ParserClientError::InvalidResponse(error.to_string()));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if response.is_some() {
                        if let Some(status) = child
                            .try_wait()
                            .map_err(|error| ParserClientError::Io(error.to_string()))?
                        {
                            if status.success() {
                                break;
                            }
                            return Err(ParserClientError::SpawnFailed(format!(
                                "python parser exited with status {status}"
                            )));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if response.is_some() {
                        break;
                    }
                    return Err(ParserClientError::InvalidResponse(
                        "python parser stream closed unexpectedly".to_string(),
                    ));
                }
            }
        }

        let response = response
            .ok_or_else(|| ParserClientError::InvalidResponse("missing response".to_string()))?;

        if response.ok {
            response
                .document
                .ok_or_else(|| ParserClientError::InvalidResponse("missing document".to_string()))
        } else {
            Err(ParserClientError::ParserFailed(response.error.unwrap_or(
                ParserError {
                    code: "parser_unknown".to_string(),
                    message: "parser returned error without details".to_string(),
                    details: None,
                },
            )))
        }
    }

    pub fn resolve_script_path(&self) -> PathBuf {
        self.runtime.resolve_script_path()
    }

    fn spawn_command(&self) -> Result<Command, ParserClientError> {
        self.runtime
            .spawn_command()
            .ok_or(ParserClientError::NotConfigured)
    }
}

pub fn python_parser_enabled() -> bool {
    match std::env::var("SEEKMIND_USE_PY_PARSER") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            normalized != "0" && normalized != "false"
        }
        Err(_) => PythonParserClient::from_env().is_configured(),
    }
}

#[allow(dead_code)]
pub fn python_parser_available() -> bool {
    PythonParserClient::from_env().is_configured()
}

#[allow(dead_code)]
pub fn python_parse_or_fallback(path: &Path) -> Option<ParsedDocument> {
    if !python_parser_enabled() {
        return None;
    }

    let client = PythonParserClient::from_env();
    client.parse_document(path).ok()
}

#[allow(dead_code)]
pub fn python_parser_config_json() -> serde_json::Value {
    let client = PythonParserClient::from_env();
    json!({
        "enabled": python_parser_enabled(),
        "available": client.is_configured(),
        "bin": client.runtime.python_bin.clone(),
        "script": client.resolve_script_path().to_string_lossy().to_string(),
        "timeout_ms": resolve_timeout_ms("SEEKMIND_PARSER_TIMEOUT_MS", None, 30_000),
    })
}

fn office_converter_path() -> Option<String> {
    office_converter_candidates()
        .into_iter()
        .find(|candidate| office_converter_works(candidate))
}

fn office_converter_works(candidate: &str) -> bool {
    if candidate.trim().is_empty() {
        return false;
    }

    Command::new(candidate)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn office_converter_config_json() -> serde_json::Value {
    let bin = office_converter_path().unwrap_or_default();
    let available = !bin.is_empty();
    let message = if available {
        "ready"
    } else {
        "LibreOffice / soffice not found"
    };

    json!({
        "enabled": true,
        "available": available,
        "bin": bin,
        "message": message,
        "platform": std::env::consts::OS,
    })
}
