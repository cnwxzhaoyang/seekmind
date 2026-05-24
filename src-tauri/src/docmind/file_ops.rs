use std::path::Path;
use std::process::{Command, Stdio};

pub fn open_file_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("文件路径不能为空".to_string());
    }

    if !Path::new(path).exists() {
        return Err(format!("文件不存在: {path}"));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path])
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

pub fn quick_look_file_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("文件路径不能为空".to_string());
    }

    if !Path::new(path).exists() {
        return Err(format!("文件不存在: {path}"));
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("qlmanage")
            .args(["-p", path])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        return Err("Quick Look 仅在 macOS 上可用".to_string());
    }
}
