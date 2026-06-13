/*
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 平台运行时路径解析，统一管理资源目录、sidecar 路径与 fastembed 缓存路径。
 */

use std::path::{Path, PathBuf};

pub const FASTEMBED_MODEL_CACHE_DIRNAME: &str = "models--Qdrant--bge-small-zh-v1.5";

pub fn env_override(names: &[&str]) -> Option<String> {
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

pub fn target_triple_suffix() -> String {
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

pub fn executable_name(base_name: &str) -> String {
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
    let executable = executable_name(base_name);
    let suffixed = format!("{base_name}-{}", target_triple_suffix());
    let suffixed_executable = executable_name(&suffixed);
    vec![suffixed_executable, executable]
}

fn candidate_paths_for_base_dir(base_dir: &Path, base_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for name in suffix_candidates(base_name) {
        paths.push(base_dir.join(&name));
        paths.push(base_dir.join(&name).join(executable_name(base_name)));
    }
    paths
}

pub fn resource_base_dirs() -> Vec<PathBuf> {
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

pub fn bundled_resource_dir(name: &str) -> Option<PathBuf> {
    for base_dir in resource_base_dirs() {
        let nested_candidate = base_dir.join("app-resources").join(name);
        if nested_candidate.exists() {
            return Some(nested_candidate);
        }

        let direct_candidate = base_dir.join(name);
        if direct_candidate.exists() {
            return Some(direct_candidate);
        }
    }
    None
}

pub fn bundled_resource_file(name: &str) -> Option<PathBuf> {
    for base_dir in resource_base_dirs() {
        let nested_candidate = base_dir.join("app-resources").join(name);
        if nested_candidate.is_file() {
            return Some(nested_candidate);
        }

        let direct_candidate = base_dir.join(name);
        if direct_candidate.is_file() {
            return Some(direct_candidate);
        }
    }
    None
}

pub fn resolve_bundled_sidecar(base_name: &str) -> Option<PathBuf> {
    let mut search_roots = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            search_roots.push(parent.to_path_buf());

            if let Some(bundle_root) = parent.parent() {
                search_roots.push(bundle_root.join("Resources"));
                search_roots.push(bundle_root.join("Resources").join("resources"));
                search_roots.push(bundle_root.join("Resources").join("app-resources"));
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        search_roots.push(cwd.join("src-tauri").join("resources"));
        search_roots.push(cwd.join("src-tauri").join("app-resources"));
    }

    for root in search_roots {
        for candidate in candidate_paths_for_base_dir(&root, base_name) {
            if candidate.is_file() {
                return Some(candidate);
            }
            if candidate.is_dir() {
                let executable = candidate.join(executable_name(base_name));
                if executable.is_file() {
                    return Some(executable);
                }
            }
        }
    }

    None
}

pub fn bundled_fastembed_cache_dir() -> Option<PathBuf> {
    bundled_resource_dir("fastembed")
}

pub fn bundled_fastembed_cache_archive() -> Option<PathBuf> {
    bundled_resource_file("fastembed-cache.tar.gz")
}

pub fn bundled_vision_ocr_dir() -> Option<PathBuf> {
    bundled_resource_dir("ocr")
}

pub fn bundled_vision_ocr_binary_path() -> Option<PathBuf> {
    let binary_name = executable_name("vision-ocr");
    bundled_vision_ocr_dir().and_then(|dir| {
        let candidate = dir.join(binary_name);
        if candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    })
}

pub fn vision_ocr_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(value) = std::env::var("SEEKMIND_VISION_OCR_BIN") {
        let candidate = PathBuf::from(value);
        if candidate.exists() {
            candidates.push(candidate);
        }
    }

    if let Some(candidate) = bundled_vision_ocr_binary_path() {
        candidates.push(candidate);
    }

    if cfg!(target_os = "macos") {
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin/vision-ocr"),
            PathBuf::from("/usr/local/bin/vision-ocr"),
            PathBuf::from("/opt/local/bin/vision-ocr"),
        ]);
    }

    candidates.push(PathBuf::from(executable_name("vision-ocr")));
    candidates
}

pub fn office_converter_candidates() -> Vec<String> {
    let mut candidates = vec![std::env::var("SEEKMIND_OFFICE_BIN").ok()];

    if cfg!(target_os = "windows") {
        candidates.extend([
            Some("soffice.exe".to_string()),
            Some("libreoffice.exe".to_string()),
            std::env::var("PROGRAMFILES")
                .ok()
                .map(|value| format!("{value}\\LibreOffice\\program\\soffice.exe")),
            std::env::var("PROGRAMFILES(X86)")
                .ok()
                .map(|value| format!("{value}\\LibreOffice\\program\\soffice.exe")),
        ]);
    } else if cfg!(target_os = "macos") {
        candidates.extend([
            Some("soffice".to_string()),
            Some("libreoffice".to_string()),
            Some("/Applications/LibreOffice.app/Contents/MacOS/soffice".to_string()),
            Some("/Applications/LibreOffice.app/Contents/MacOS/libreoffice".to_string()),
        ]);
    } else {
        candidates.extend([Some("soffice".to_string()), Some("libreoffice".to_string())]);
    }

    candidates.into_iter().flatten().collect()
}

pub fn writable_fastembed_cache_dir() -> PathBuf {
    if let Some(configured) =
        env_override(&["SEEKMIND_FASTEMBED_CACHE_DIR", "SeekMind_FASTEMBED_CACHE_DIR"])
    {
        return PathBuf::from(configured);
    }

    let base = dirs::cache_dir()
        .or_else(dirs::data_local_dir)
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("com.zhaoyang.seekmind").join("fastembed")
}

pub fn fastembed_model_cache_dir(base_dir: &Path) -> PathBuf {
    base_dir.join(FASTEMBED_MODEL_CACHE_DIRNAME)
}
