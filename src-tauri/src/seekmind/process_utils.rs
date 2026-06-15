/*
 * @author MorningSun
 * @CreatedDate 2026/06/15
 * @Description Windows child process helpers for SeekMind, including hidden PowerShell launch.
 */

use std::path::PathBuf;
use std::process::{Command, Output};

#[cfg(target_os = "windows")]
fn windows_powershell_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(system_root) = std::env::var_os("SystemRoot").map(PathBuf::from) {
        candidates.push(
            system_root
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
        );
        candidates.push(
            system_root
                .join("SysWOW64")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
        );
    }

    candidates.push(PathBuf::from("powershell.exe"));
    candidates
}

#[cfg(target_os = "windows")]
pub fn configure_hidden_child_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    // Hide transient consoles from parser, Office, and embedding child processes on Windows.
    command.creation_flags(0x0800_0000);
}

#[cfg(not(target_os = "windows"))]
pub fn configure_hidden_child_process(_command: &mut Command) {
    // Non-Windows platforms do not show a transient console window for these child processes.
}

#[cfg(target_os = "windows")]
pub fn run_hidden_powershell_script(script: &str) -> Result<(PathBuf, Output), String> {
    let powershell = windows_powershell_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "Windows PowerShell executable not found".to_string())?;

    let mut command = Command::new(&powershell);
    configure_hidden_child_process(&mut command);

    let output = command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .map_err(|error| error.to_string())?;

    Ok((powershell, output))
}

#[cfg(not(target_os = "windows"))]
pub fn run_hidden_powershell_script(_script: &str) -> Result<(PathBuf, Output), String> {
    Err("hidden PowerShell execution is only available on Windows".to_string())
}
