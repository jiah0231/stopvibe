use anyhow::Result;
use std::collections::HashSet;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use stopvibe_common::BlockTarget;
use tracing::info;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

/// Scan running processes and terminate any that match active block targets.
/// Returns the number of processes killed.
pub fn scan_and_kill(targets: &[&BlockTarget]) -> Result<u32> {
    let mut killed = 0u32;

    // Collect exe names to block (lowercase for comparison)
    let blocked_exes: HashSet<String> = targets
        .iter()
        .flat_map(|t| t.exe_names.iter())
        .map(|s| s.to_lowercase())
        .collect();

    // Collect cmdline patterns
    let cmdline_patterns: Vec<&str> = targets
        .iter()
        .flat_map(|t| t.cmdline_patterns.iter())
        .map(|s| s.as_str())
        .collect();

    // Use tasklist-style enumeration via CreateToolhelp32Snapshot
    let snapshot = unsafe {
        windows::Win32::System::Diagnostics::ToolHelp::CreateToolhelp32Snapshot(
            windows::Win32::System::Diagnostics::ToolHelp::TH32CS_SNAPPROCESS,
            0,
        )
    }?;

    let mut entry = windows::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32W {
        dwSize: std::mem::size_of::<windows::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32W>(
        ) as u32,
        ..Default::default()
    };

    let ok = unsafe {
        windows::Win32::System::Diagnostics::ToolHelp::Process32FirstW(snapshot, &mut entry)
    };

    if ok.is_err() {
        unsafe { CloseHandle(snapshot).ok() };
        return Ok(0);
    }

    loop {
        let exe_name = OsString::from_wide(
            &entry.szExeFile[..entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len())],
        )
        .to_string_lossy()
        .to_lowercase();

        let pid = entry.th32ProcessID;

        // Check exe name match
        if blocked_exes.contains(&exe_name) {
            if kill_process(pid) {
                info!("Killed blocked process: {} (PID {})", exe_name, pid);
                killed += 1;
            }
        }
        // Check script/package-manager hosts for tools launched through npm, npx,
        // Python entrypoints, or similar wrappers.
        else if !cmdline_patterns.is_empty() && is_script_host(&exe_name) {
            if let Some(cmdline) = get_process_cmdline(pid) {
                let cmdline_lower = cmdline.to_lowercase();
                for pattern in &cmdline_patterns {
                    if cmdline_lower.contains(&pattern.to_lowercase()) {
                        if kill_process(pid) {
                            info!(
                                "Killed process by cmdline match: {} (PID {}, pattern: {})",
                                exe_name, pid, pattern
                            );
                            killed += 1;
                        }
                        break;
                    }
                }
            }
        }

        let next = unsafe {
            windows::Win32::System::Diagnostics::ToolHelp::Process32NextW(snapshot, &mut entry)
        };
        if next.is_err() {
            break;
        }
    }

    unsafe { CloseHandle(snapshot).ok() };
    Ok(killed)
}

fn kill_process(pid: u32) -> bool {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) };
    match handle {
        Ok(h) => {
            let result = unsafe { TerminateProcess(h, 1) };
            unsafe { CloseHandle(h).ok() };
            result.is_ok()
        }
        Err(_) => false,
    }
}

fn get_process_cmdline(pid: u32) -> Option<String> {
    // Use WMI via command or NtQueryInformationProcess
    // For simplicity, use a quick wmic query
    let output = std::process::Command::new("wmic")
        .args([
            "process",
            "where",
            &format!("ProcessId={}", pid),
            "get",
            "CommandLine",
            "/value",
        ])
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(val) = line.strip_prefix("CommandLine=") {
            return Some(val.to_string());
        }
    }
    None
}

fn is_script_host(exe_name: &str) -> bool {
    matches!(
        exe_name,
        "python.exe"
            | "python3.exe"
            | "py.exe"
            | "node.exe"
            | "npm.exe"
            | "npx.exe"
            | "pnpm.exe"
            | "yarn.exe"
            | "bun.exe"
            | "uv.exe"
    )
}
