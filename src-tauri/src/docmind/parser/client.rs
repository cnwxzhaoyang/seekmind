use std::fmt::{Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;

use super::types::{ParsedDocument, ParserError, ParserOptions, ParserRequest, ParserResponse};

#[derive(Debug, Clone)]
pub struct PythonParserClient {
    python_bin: String,
    script_path: PathBuf,
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
            Self::InvalidResponse(error) => write!(f, "python parser returned invalid response: {error}"),
            Self::ParserFailed(error) => write!(f, "python parser failed: {} ({})", error.message, error.code),
        }
    }
}

impl std::error::Error for ParserClientError {}

impl PythonParserClient {
    pub fn from_env() -> Self {
        let python_bin = std::env::var("DOCMIND_PARSER_BIN").unwrap_or_else(|_| "python3".to_string());
        let script_path = std::env::var("DOCMIND_PARSER_SCRIPT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("parser/docmind_parser/__main__.py"));
        let timeout_ms = std::env::var("DOCMIND_PARSER_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000);

        Self {
            python_bin,
            script_path,
            timeout: Duration::from_millis(timeout_ms),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.resolve_script_path().exists()
    }

    pub fn parse_document(&self, path: &Path) -> Result<ParsedDocument, ParserClientError> {
        if !self.is_configured() {
            return Err(ParserClientError::NotConfigured);
        }

        let script_path = self.resolve_script_path();

        let request = ParserRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            command: "parse_document".to_string(),
            path: path.to_string_lossy().to_string(),
            options: ParserOptions::default(),
        };

        let payload = serde_json::to_vec(&request)
            .map_err(|error| ParserClientError::Io(error.to_string()))?;

        let mut child = Command::new(&self.python_bin)
            .arg(&script_path)
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

        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait().map_err(|error| ParserClientError::Io(error.to_string()))? {
                Some(_) => break,
                None => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        return Err(ParserClientError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }

        let output = child
            .wait_with_output()
            .map_err(|error| ParserClientError::Io(error.to_string()))?;

        let response: Result<ParserResponse, _> = serde_json::from_slice(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        match response {
            Ok(response) => {
                if response.request_id != request.request_id {
                    return Err(ParserClientError::InvalidResponse("request_id mismatch".to_string()));
                }

                if response.ok {
                    response
                        .document
                        .ok_or_else(|| ParserClientError::InvalidResponse("missing document".to_string()))
                } else {
                    if !stderr.is_empty() {
                        eprintln!("[docmind:parser] python stderr: {stderr}");
                    }
                    Err(ParserClientError::ParserFailed(
                        response.error.unwrap_or(ParserError {
                            code: "parser_unknown".to_string(),
                            message: "parser returned error without details".to_string(),
                            details: None,
                        }),
                    ))
                }
            }
            Err(error) => {
                if output.status.success() {
                    Err(ParserClientError::InvalidResponse(error.to_string()))
                } else {
                    Err(ParserClientError::SpawnFailed(if stderr.is_empty() {
                        format!("python parser exited with non-zero status: {error}")
                    } else {
                        stderr
                    }))
                }
            }
        }
    }

    pub fn resolve_script_path(&self) -> PathBuf {
        if self.script_path.is_absolute() {
            return self.script_path.clone();
        }

        let candidates = [
            std::env::current_dir().ok().map(|cwd| cwd.join(&self.script_path)),
            std::env::current_dir()
                .ok()
                .map(|cwd| cwd.join("src-tauri").join(&self.script_path)),
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|parent| parent.join(&self.script_path))),
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(&self.script_path)),
        ];

        for candidate in candidates.into_iter().flatten() {
            if candidate.exists() {
                return candidate;
            }
        }

        self.script_path.clone()
    }
}

pub fn python_parser_enabled() -> bool {
    matches!(std::env::var("DOCMIND_USE_PY_PARSER"), Ok(value) if value != "0")
}

#[allow(dead_code)]
pub fn python_parser_available() -> bool {
    PythonParserClient::from_env().is_configured()
}

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
        "bin": std::env::var("DOCMIND_PARSER_BIN").unwrap_or_else(|_| "python3".to_string()),
        "script": client.resolve_script_path().to_string_lossy().to_string(),
        "timeout_ms": std::env::var("DOCMIND_PARSER_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(30_000),
    })
}
