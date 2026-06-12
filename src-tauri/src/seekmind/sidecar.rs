/*
 * @author MorningSun
 * @CreatedDate 2026/06/06
 * @Description SeekMind sidecar and bundled helper environment wiring.
 */

use std::path::PathBuf;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::symlink;

use crate::seekmind::vision_ocr::{bundled_vision_ocr_binary, default_vision_ocr_languages};
use crate::seekmind::storage::types::NetworkProxySettings;

fn env_override(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(value) = std::env::var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn target_triple_suffix() -> String {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin".to_string(),
        ("macos", "x86_64") => "x86_64-apple-darwin".to_string(),
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu".to_string(),
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu".to_string(),
        ("windows", "x86_64") => "x86_64-pc-windows-msvc".to_string(),
        ("windows", "aarch64") => "aarch64-pc-windows-msvc".to_string(),
        (os, arch) => format!("{arch}-unknown-{os}"),
    }
}

fn executable_name(base_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("{base_name}.exe")
    }

    #[cfg(not(target_os = "windows"))]
    {
        base_name.to_string()
    }
}

fn suffix_candidates(base_name: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let executable = executable_name(base_name);
    let suffixed = format!("{base_name}-{}", target_triple_suffix());
    let suffixed_executable = executable_name(&suffixed);

    candidates.push(suffixed_executable);
    candidates.push(executable);

    candidates
}

fn candidate_paths_for_base_dir(base_dir: PathBuf, base_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for name in suffix_candidates(base_name) {
        paths.push(base_dir.join(&name));
        paths.push(base_dir.join(&name).join(executable_name(base_name)));
    }
    paths
}

fn candidate_paths(base_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let current_exe = std::env::current_exe().ok();
    let current_dir = std::env::current_dir().ok();

    if let Some(exe) = current_exe.as_ref() {
        if let Some(parent) = exe.parent() {
            paths.extend(candidate_paths_for_base_dir(
                parent.to_path_buf(),
                base_name,
            ));

            if let Some(bundle_root) = parent.parent() {
                let resources_dir = bundle_root.join("Resources");
                paths.extend(candidate_paths_for_base_dir(
                    resources_dir.clone(),
                    base_name,
                ));
                paths.extend(candidate_paths_for_base_dir(
                    resources_dir.join("resources"),
                    base_name,
                ));
                paths.extend(candidate_paths_for_base_dir(
                    resources_dir.join("app-resources"),
                    base_name,
                ));
            }
        }
    }

    if let Some(cwd) = current_dir.as_ref() {
        let resources_dir = cwd.join("src-tauri").join("resources");
        paths.extend(candidate_paths_for_base_dir(resources_dir, base_name));
        let app_resources_dir = cwd.join("src-tauri").join("app-resources");
        paths.extend(candidate_paths_for_base_dir(app_resources_dir, base_name));
    }

    paths
}

pub fn resolve_bundled_sidecar(base_name: &str) -> Option<PathBuf> {
    for candidate in candidate_paths(base_name) {
        if candidate.is_file() {
            return Some(candidate);
        }
        if candidate.is_dir() {
            let executable = candidate.join(executable_name(base_name));
            if executable.exists() {
                return Some(executable);
            }
        }
    }
    None
}

pub fn configure_sidecar_command(command: &mut Command) {
    let cache_dir = ensure_fastembed_cache_dir();
    eprintln!(
        "[SeekMind] semantic sidecar cache dir={}",
        cache_dir.display()
    );
    command.env("SEEKMIND_FASTEMBED_CACHE_DIR", &cache_dir);
    command.env("HF_HOME", cache_dir.join("huggingface"));

    // 修复：沙盒运行时不能再依赖系统路径下的 OCR 进程，这里改为注入打包内 Vision OCR helper。
    if let Some(binary) = bundled_vision_ocr_binary() {
        let languages = default_vision_ocr_languages();
        eprintln!(
            "[SeekMind] using bundled Vision OCR helper: bin={}, langs={}",
            binary.display(),
            languages.join(",")
        );
        command.env("SEEKMIND_VISION_OCR_BIN", &binary);
        command.env("SEEKMIND_VISION_OCR_LANGS", languages.join(","));
    } else {
        eprintln!("[SeekMind] bundled Vision OCR helper not found; PDF OCR may be unavailable in sandbox");
    }
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

    let has_model_cache = cache_dir.join("models--Qdrant--bge-small-zh-v1.5").exists();
    if has_model_cache {
        eprintln!(
            "[SeekMind] fastembed cache hit dir={}",
            cache_dir.display()
        );
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
    let model_cache_dir = cache_dir.join("models--Qdrant--bge-small-zh-v1.5");
    eprintln!(
        "[SeekMind] fastembed runtime cache prepared dir={} model_cache_exists={}",
        cache_dir.display(),
        model_cache_dir.exists()
    );
}

pub fn log_fastembed_cache_diagnostics() {
    let cache_dir = writable_fastembed_cache_dir();
    let model_cache_dir = cache_dir.join("models--Qdrant--bge-small-zh-v1.5");
    let model_cache_exists = model_cache_dir.exists();
    let configured_override =
        env_override(&["SEEKMIND_FASTEMBED_CACHE_DIR", "SeekMind_FASTEMBED_CACHE_DIR"]);
    let bundled_cache_dir = bundled_fastembed_cache_dir();
    let bundled_cache_archive = bundled_fastembed_cache_archive();

    eprintln!(
        "[SeekMind] fastembed cache diagnostics dir={} model_cache_exists={} env_override={} bundled_dir={} bundled_archive={}",
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

fn writable_fastembed_cache_dir() -> PathBuf {
    if let Some(configured) =
        env_override(&["SEEKMIND_FASTEMBED_CACHE_DIR", "SeekMind_FASTEMBED_CACHE_DIR"])
    {
        // 修复：开发态 warmup 与 tauri dev 必须共享同一个显式模型缓存目录，不能在 sidecar 内再次强制改写到系统 cache。
        return PathBuf::from(configured);
    }

    let base = dirs::cache_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("com.zhaoyang.seekmind").join("fastembed")
}

fn bundled_fastembed_cache_dir() -> Option<PathBuf> {
    for base_dir in resource_base_dirs() {
        let candidate = base_dir.join("app-resources").join("fastembed");
        if candidate.exists() {
            return Some(candidate);
        }

        let candidate = base_dir.join("fastembed");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn bundled_fastembed_cache_archive() -> Option<PathBuf> {
    for base_dir in resource_base_dirs() {
        let candidate = base_dir
            .join("app-resources")
            .join("fastembed-cache.tar.gz");
        if candidate.is_file() {
            return Some(candidate);
        }

        let candidate = base_dir.join("fastembed-cache.tar.gz");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn extract_fastembed_cache_archive(archive: &PathBuf, target: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    let status = Command::new("/usr/bin/tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(target)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("tar exited with status {status}"),
        ))
    }
}

fn resource_base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("app-resources"));

            if let Some(bundle_root) = parent.parent() {
                dirs.push(bundle_root.join("Resources"));
                dirs.push(bundle_root.join("debug").join("app-resources"));
                dirs.push(bundle_root.join("release").join("app-resources"));
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("src-tauri").join("app-resources"));
        dirs.push(cwd.join("src-tauri").join("target").join("debug").join("app-resources"));
        dirs.push(cwd.join("src-tauri").join("target").join("release").join("app-resources"));
    }

    dirs
}

fn copy_dir_missing(source: &PathBuf, target: &PathBuf) -> std::io::Result<()> {
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
fn copy_symlink(source: &PathBuf, target: &PathBuf) -> std::io::Result<()> {
    let link_target = std::fs::read_link(source)?;
    symlink(link_target, target)
}

#[cfg(not(unix))]
fn copy_symlink(source: &PathBuf, target: &PathBuf) -> std::io::Result<()> {
    let resolved = std::fs::canonicalize(source)?;
    std::fs::copy(resolved, target).map(|_| ())
}
