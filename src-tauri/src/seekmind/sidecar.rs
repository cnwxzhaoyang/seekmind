/*
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind sidecar 启动配置、模型缓存准备与运行时诊断。
 */

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[cfg(unix)]
use std::os::unix::fs::symlink;

use flate2::read::GzDecoder;
use tar::Archive;

use crate::seekmind::process_utils::configure_hidden_child_process;
use crate::seekmind::runtime_paths::{
    bundled_fastembed_cache_archive, bundled_fastembed_cache_dir, bundled_vision_ocr_binary_path,
    env_override, fastembed_model_cache_dir, writable_fastembed_cache_dir,
};
use crate::seekmind::storage::types::NetworkProxySettings;
use crate::seekmind::vision_ocr::default_vision_ocr_languages;

pub use crate::seekmind::runtime_paths::resolve_bundled_sidecar;

#[derive(Debug, Clone)]
pub struct PythonSidecarRuntime {
    pub python_bin: String,
    pub script_path: PathBuf,
    pub bundled_sidecar: Option<PathBuf>,
}

impl PythonSidecarRuntime {
    pub fn from_env(default_script_path: &str) -> Self {
        let python_bin =
            std::env::var("SEEKMIND_PARSER_BIN").unwrap_or_else(|_| default_python_bin());
        let script_path = std::env::var("SEEKMIND_PARSER_SCRIPT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(default_script_path));
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

        Self {
            python_bin,
            script_path,
            bundled_sidecar,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.bundled_sidecar.is_some() || self.resolve_script_path().exists()
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

    pub fn spawn_command(&self) -> Option<Command> {
        if let Some(path) = &self.bundled_sidecar {
            let mut command = Command::new(path);
            configure_sidecar_command(&mut command);
            return Some(command);
        }

        let script_path = self.resolve_script_path();
        if !script_path.exists() {
            return None;
        }

        let mut command = Command::new(&self.python_bin);
        command.arg(script_path);
        configure_sidecar_command(&mut command);
        Some(command)
    }
}

pub fn configure_sidecar_command(command: &mut Command) {
    configure_hidden_child_process(command);

    let cache_dir = ensure_fastembed_cache_dir();
    eprintln!(
        "[SeekMind] semantic sidecar cache dir={}",
        cache_dir.display()
    );
    command.env("SEEKMIND_FASTEMBED_CACHE_DIR", &cache_dir);
    command.env("HF_HOME", cache_dir.join("huggingface"));

    // 修复：统一通过 runtime_paths 注入打包内 OCR helper，避免平台切换时再次散落一套路径解析。
    if let Some(binary) = bundled_vision_ocr_binary_path() {
        let languages = default_vision_ocr_languages();
        eprintln!(
            "[SeekMind] using bundled Vision OCR helper: bin={}, langs={}",
            binary.display(),
            languages.join(",")
        );
        command.env("SEEKMIND_VISION_OCR_BIN", &binary);
        command.env("SEEKMIND_VISION_OCR_LANGS", languages.join(","));
    } else {
        eprintln!(
            "[SeekMind] bundled Vision OCR helper not found; PDF OCR may be unavailable in sandbox"
        );
    }
}

pub fn default_python_bin() -> String {
    if cfg!(target_os = "windows") {
        static WINDOWS_PYTHON_BIN: OnceLock<String> = OnceLock::new();
        WINDOWS_PYTHON_BIN
            .get_or_init(|| {
                if let Some(python_bin) = detect_windows_python_bin() {
                    eprintln!(
                        "[SeekMind] using detected Windows Python runtime for sidecar: {}",
                        python_bin
                    );
                    return python_bin;
                }

                // 修复：至少回退到 py launcher，避免 Store alias 的 python.exe 让 sidecar 误判不可用。
                let fallback = "py".to_string();
                eprintln!(
                    "[SeekMind] using fallback Windows Python launcher `py` for sidecar runtime"
                );
                fallback
            })
            .clone()
    } else {
        "python3".to_string()
    }
}

#[cfg(target_os = "windows")]
fn detect_windows_python_bin() -> Option<String> {
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let mut candidates = Vec::new();

    if let Some(base) = local_app_data {
        for version in [
            "Python314",
            "Python313",
            "Python312",
            "Python311",
            "Python310",
        ] {
            candidates.push(
                base.join("Programs")
                    .join("Python")
                    .join(version)
                    .join("python.exe"),
            );
        }
    }

    if let Some(base) = user_profile {
        candidates.push(base.join("Miniconda3").join("python.exe"));
        candidates.push(base.join("anaconda3").join("python.exe"));
    }

    for candidate in candidates
        .into_iter()
        .filter(|candidate| candidate.exists())
    {
        // 修复：多版本 Python 并存时，只选择已安装 fastembed 的解释器，避免运行时继续误落到空环境。
        let mut command = Command::new(&candidate);
        configure_hidden_child_process(&mut command);
        let import_ready = command
            .args(["-c", "import fastembed"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if import_ready {
            return Some(candidate.display().to_string());
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
fn detect_windows_python_bin() -> Option<String> {
    None
}

pub fn resolve_timeout_ms(primary_env: &str, fallback_env: Option<&str>, default_ms: u64) -> u64 {
    std::env::var(primary_env)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            fallback_env.and_then(|name| {
                std::env::var(name)
                    .ok()
                    .and_then(|value| value.parse::<u64>().ok())
            })
        })
        .unwrap_or(default_ms)
}

pub fn apply_network_proxy_environment(settings: Option<&NetworkProxySettings>) {
    let proxy_url = settings
        .filter(|setting| setting.enabled)
        .map(|setting| setting.proxy_url.trim())
        .filter(|value| !value.is_empty());

    apply_proxy_env_var("HTTP_PROXY", proxy_url);
    apply_proxy_env_var("HTTPS_PROXY", proxy_url);
    apply_proxy_env_var("ALL_PROXY", proxy_url);
    apply_proxy_env_var("http_proxy", proxy_url);
    apply_proxy_env_var("https_proxy", proxy_url);
    apply_proxy_env_var("all_proxy", proxy_url);
}

fn apply_proxy_env_var(name: &str, value: Option<&str>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

fn ensure_fastembed_cache_dir() -> PathBuf {
    let cache_dir = writable_fastembed_cache_dir();
    if let Err(error) = std::fs::create_dir_all(&cache_dir) {
        eprintln!(
            "[SeekMind] failed to create FastEmbed cache dir {}: {error}",
            cache_dir.display()
        );
        return cache_dir;
    }

    let model_cache_dir = fastembed_model_cache_dir(&cache_dir);
    if model_cache_dir.exists() {
        eprintln!("[SeekMind] fastembed cache hit dir={}", cache_dir.display());
    } else if let Some(archive) = bundled_fastembed_cache_archive() {
        match extract_fastembed_cache_archive(&archive, &cache_dir) {
            Ok(()) => {
                eprintln!(
                    "[SeekMind] restored fastembed cache from archive {} to {}",
                    archive.display(),
                    cache_dir.display()
                );
            }
            Err(error) => {
                eprintln!(
                    "[SeekMind] failed to extract bundled FastEmbed cache from {} to {}: {error}",
                    archive.display(),
                    cache_dir.display()
                );
            }
        }
    } else if let Some(bundled_cache) = bundled_fastembed_cache_dir() {
        match copy_dir_missing(&bundled_cache, &cache_dir) {
            Ok(()) => {
                eprintln!(
                    "[SeekMind] restored fastembed cache from dir {} to {}",
                    bundled_cache.display(),
                    cache_dir.display()
                );
            }
            Err(error) => {
                eprintln!(
                    "[SeekMind] failed to copy bundled FastEmbed cache from {} to {}: {error}",
                    bundled_cache.display(),
                    cache_dir.display()
                );
            }
        }
    } else {
        eprintln!(
            "[SeekMind] fastembed cache miss dir={} and no bundled cache source found",
            cache_dir.display()
        );
    }

    cache_dir
}

pub fn prepare_fastembed_cache_for_runtime() {
    let cache_dir = ensure_fastembed_cache_dir();
    let model_cache_dir = fastembed_model_cache_dir(&cache_dir);
    eprintln!(
        "[SeekMind] fastembed runtime cache prepared dir={} model_cache_exists={}",
        cache_dir.display(),
        model_cache_dir.exists()
    );
}

pub fn log_fastembed_cache_diagnostics() {
    let cache_dir = writable_fastembed_cache_dir();
    let model_cache_exists = fastembed_model_cache_dir(&cache_dir).exists();
    let configured_override = env_override(&[
        "SEEKMIND_FASTEMBED_CACHE_DIR",
        "SeekMind_FASTEMBED_CACHE_DIR",
    ]);
    let bundled_cache_dir = bundled_fastembed_cache_dir();
    let bundled_cache_archive = bundled_fastembed_cache_archive();
    let parser_sidecar = resolve_bundled_sidecar("seekmind-parser");
    let ocr_binary = bundled_vision_ocr_binary_path();

    eprintln!(
        "[SeekMind] runtime paths platform={} arch={} parser_sidecar={} parser_sidecar_exists={} ocr_binary={} ocr_binary_exists={} fastembed_cache_dir={} model_cache_exists={} env_override={} bundled_fastembed_dir={} bundled_fastembed_archive={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        parser_sidecar
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        parser_sidecar.as_ref().is_some_and(|path| path.exists()),
        ocr_binary
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        ocr_binary.as_ref().is_some_and(|path| path.exists()),
        cache_dir.display(),
        model_cache_exists,
        configured_override.as_deref().unwrap_or(""),
        bundled_cache_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        bundled_cache_archive
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
}

fn extract_fastembed_cache_archive(archive: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    let file = std::fs::File::open(archive)?;
    let decoder = GzDecoder::new(file);
    let mut tar = Archive::new(decoder);
    tar.unpack(target)
}

fn copy_dir_missing(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());

        if target_path.exists() {
            continue;
        }

        let metadata = std::fs::symlink_metadata(&source_path)?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            copy_dir_missing(&source_path, &target_path)?;
        } else if file_type.is_symlink() {
            copy_symlink(&source_path, &target_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&source_path, &target_path)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    let link_target = std::fs::read_link(source)?;
    symlink(link_target, target)
}

#[cfg(not(unix))]
fn copy_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    let resolved = std::fs::canonicalize(source)?;
    std::fs::copy(resolved, target).map(|_| ())
}
