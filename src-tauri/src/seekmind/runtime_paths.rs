/*
 * @author MorningSun
 * @CreatedDate 2026/06/13
 * @Description SeekMind 平台运行时路径解析，统一管理资源目录、sidecar 路径与 fastembed 缓存路径。
 */

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::seekmind::process_utils::configure_hidden_child_process;
#[cfg(target_os = "windows")]
use crate::seekmind::process_utils::run_hidden_powershell_script;

pub const FASTEMBED_MODEL_CACHE_DIRNAME: &str = "models--Qdrant--bge-small-zh-v1.5";

#[derive(Debug, Clone)]
pub struct OfficeRuntime {
    pub available: bool,
    pub kind: String,
    pub bin: String,
    pub message: String,
    pub platform: String,
}

static OFFICE_RUNTIME: OnceLock<OfficeRuntime> = OnceLock::new();

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
        dirs.push(
            cwd.join("src-tauri")
                .join("target")
                .join("debug")
                .join("app-resources"),
        );
        dirs.push(
            cwd.join("src-tauri")
                .join("target")
                .join("release")
                .join("app-resources"),
        );
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

fn office_converter_candidates() -> Vec<String> {
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

fn libreoffice_converter_path() -> Option<String> {
    office_converter_candidates()
        .into_iter()
        .find(|candidate| office_converter_works(candidate))
}

fn office_converter_works(candidate: &str) -> bool {
    if candidate.trim().is_empty() {
        return false;
    }

    let mut command = Command::new(candidate);
    configure_hidden_child_process(&mut command);
    command
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn detect_windows_office_runtime() -> Option<OfficeRuntime> {
    let probe = r#"
$apps = @()
try {
  $word = New-Object -ComObject Word.Application
  $word.DisplayAlerts = 0
  $word.Quit()
  $apps += 'Word'
} catch {}
try {
  $powerpoint = New-Object -ComObject PowerPoint.Application
  $powerpoint.Quit()
  $apps += 'PowerPoint'
} catch {}
try {
  $excel = New-Object -ComObject Excel.Application
  $excel.DisplayAlerts = $false
  $excel.Quit()
  $apps += 'Excel'
} catch {}
if ($apps.Count -gt 0) {
  [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
  Write-Output ($apps -join ', ')
}
"#;

    let (powershell, output) = run_hidden_powershell_script(probe).ok()?;

    if !output.status.success() {
        eprintln!(
            "[SeekMind] office runtime COM probe failed powershell={} stderr={}",
            powershell.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }

    let apps = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if apps.is_empty() {
        return None;
    }

    Some(OfficeRuntime {
        available: true,
        kind: "windows-office-com".to_string(),
        bin: powershell.display().to_string(),
        message: format!("Microsoft Office via PowerShell COM ({apps})"),
        platform: std::env::consts::OS.to_string(),
    })
}

#[cfg(target_os = "macos")]
fn detect_macos_office_runtime() -> Option<OfficeRuntime> {
    let textutil = PathBuf::from("/usr/bin/textutil");
    if textutil.is_file() {
        return Some(OfficeRuntime {
            available: true,
            kind: "macos-textutil".to_string(),
            bin: textutil.display().to_string(),
            message: "macOS textutil + native OOXML extraction".to_string(),
            platform: std::env::consts::OS.to_string(),
        });
    }
    None
}

fn unavailable_office_runtime() -> OfficeRuntime {
    OfficeRuntime {
        available: false,
        kind: "unavailable".to_string(),
        bin: String::new(),
        // 修复：Office runtime 不存在时只是降级到有限文本提取，不应让状态文案看起来像主流程失败。
        message: "No compatible Office runtime detected; legacy .doc/.ppt files will fall back to limited text extraction"
            .to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

fn detect_office_runtime() -> OfficeRuntime {
    let runtime = if let Some(bin) = libreoffice_converter_path() {
        OfficeRuntime {
            available: true,
            kind: "libreoffice".to_string(),
            bin: bin.clone(),
            message: "LibreOffice / soffice ready".to_string(),
            platform: std::env::consts::OS.to_string(),
        }
    } else {
        #[cfg(target_os = "windows")]
        {
            detect_windows_office_runtime().unwrap_or_else(unavailable_office_runtime)
        }
        #[cfg(target_os = "macos")]
        {
            detect_macos_office_runtime().unwrap_or_else(unavailable_office_runtime)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            unavailable_office_runtime()
        }
    };

    eprintln!(
        "[SeekMind] office runtime platform={} kind={} available={} bin={} message={}",
        runtime.platform, runtime.kind, runtime.available, runtime.bin, runtime.message
    );
    runtime
}

pub fn office_runtime() -> OfficeRuntime {
    OFFICE_RUNTIME.get_or_init(detect_office_runtime).clone()
}

pub fn writable_fastembed_cache_dir() -> PathBuf {
    if let Some(configured) = env_override(&[
        "SEEKMIND_FASTEMBED_CACHE_DIR",
        "SeekMind_FASTEMBED_CACHE_DIR",
    ]) {
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
