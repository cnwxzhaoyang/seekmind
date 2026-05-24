use std::path::PathBuf;

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
            paths.extend(candidate_paths_for_base_dir(parent.to_path_buf(), base_name));

            if let Some(bundle_root) = parent.parent() {
                let resources_dir = bundle_root.join("Resources");
                paths.extend(candidate_paths_for_base_dir(resources_dir.clone(), base_name));
                paths.extend(candidate_paths_for_base_dir(resources_dir.join("resources"), base_name));
                paths.extend(candidate_paths_for_base_dir(resources_dir.join("app-resources"), base_name));
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
