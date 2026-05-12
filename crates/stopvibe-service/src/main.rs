#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod blocker;
mod ipc;
mod monitor;
mod protection;
mod state;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use state::StateManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use stopvibe_common::{BlockSession, BlockTarget, SERVICE_NAME};
use tracing::{error, info};
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState as WinServiceState,
    ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
const MAX_DURATION_MINUTES: u64 = 24 * 60;

pub struct ServiceState {
    state_manager: StateManager,
    session: Option<BlockSession>,
    running: bool,
}

impl ServiceState {
    pub fn new() -> Result<Self> {
        let state_manager = StateManager::new()?;
        let session = state_manager.load_session()?;
        Ok(Self {
            state_manager,
            session,
            running: true,
        })
    }

    pub fn start_blocking(
        &mut self,
        duration_minutes: u64,
        targets: Vec<BlockTarget>,
    ) -> Result<()> {
        if duration_minutes == 0 || duration_minutes > MAX_DURATION_MINUTES {
            bail!(
                "Duration must be between 1 and {} minutes",
                MAX_DURATION_MINUTES
            );
        }

        if self.session.as_ref().map_or(false, |s| s.is_active()) {
            bail!("A blocking session is already active");
        }

        if self.session.is_some() {
            self.remove_blocks();
        }

        let targets = sanitize_requested_targets(targets)?;
        let now = Utc::now();
        let end = now
            + chrono::Duration::minutes(
                i64::try_from(duration_minutes).context("Duration is too large")?,
            );

        let session = BlockSession {
            start_time: now,
            end_time: end,
            targets,
        };

        self.state_manager.save_session(&session)?;

        let enabled_targets: Vec<&BlockTarget> =
            session.targets.iter().filter(|t| t.enabled).collect();
        if let Err(e) = blocker::apply_ifeo_blocks(&enabled_targets) {
            let _ = self.state_manager.clear_session();
            return Err(e).context("Failed to apply IFEO blocks");
        }

        if let Err(e) = protection::lock_down(self.state_manager.state_path()) {
            let _ = blocker::remove_ifeo_blocks(&enabled_targets);
            let _ = protection::unlock(self.state_manager.state_path());
            let _ = self.state_manager.clear_session();
            return Err(e).context("Failed to apply self-protection");
        }

        self.session = Some(session);
        info!("Blocking session started for {} minutes", duration_minutes);
        Ok(())
    }

    pub fn current_session(&self) -> Option<BlockSession> {
        self.session.clone()
    }

    fn check_expiry(&mut self) {
        if let Some(ref session) = self.session {
            if !session.is_active() {
                info!("Blocking session expired, removing blocks");
                self.remove_blocks();
            }
        }
    }

    fn remove_blocks(&mut self) {
        if let Some(ref session) = self.session {
            let enabled_targets: Vec<&BlockTarget> =
                session.targets.iter().filter(|t| t.enabled).collect();
            if let Err(e) = blocker::remove_ifeo_blocks(&enabled_targets) {
                error!("Failed to remove IFEO blocks: {}", e);
            }
        }
        if let Err(e) = protection::unlock(self.state_manager.state_path()) {
            error!("Failed to remove protection: {}", e);
        }
        if let Err(e) = self.state_manager.clear_session() {
            error!("Failed to clear session state: {}", e);
        }
        self.session = None;
    }

    fn restore_on_boot(&mut self) {
        if let Some(ref session) = self.session {
            if session.is_active() {
                info!("Restoring blocking session after restart");
                let enabled_targets: Vec<&BlockTarget> =
                    session.targets.iter().filter(|t| t.enabled).collect();
                if let Err(e) = blocker::apply_ifeo_blocks(&enabled_targets) {
                    error!("Failed to restore IFEO blocks: {}", e);
                }
                if let Err(e) = protection::lock_down(self.state_manager.state_path()) {
                    error!("Failed to restore protection: {}", e);
                }
            } else {
                self.remove_blocks();
            }
        }
    }
}

fn sanitize_requested_targets(requested: Vec<BlockTarget>) -> Result<Vec<BlockTarget>> {
    let defaults = stopvibe_common::default_targets();
    let default_names: Vec<String> = defaults.iter().map(|t| normalize_name(&t.name)).collect();
    let mut requested_enabled = HashMap::new();

    for target in requested {
        let key = normalize_name(&target.name);
        if !default_names.contains(&key) {
            bail!("Unknown block target: {}", target.name);
        }
        requested_enabled.insert(key, target.enabled);
    }

    let sanitized: Vec<BlockTarget> = defaults
        .into_iter()
        .map(|mut target| {
            target.enabled = requested_enabled
                .get(&normalize_name(&target.name))
                .copied()
                .unwrap_or(false);
            target
        })
        .collect();

    if !sanitized.iter().any(|target| target.enabled) {
        bail!("Select at least one target");
    }

    Ok(sanitized)
}

fn normalize_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--install" => return install_service(),
            "--stub" => {
                run_stub(&args[2..]);
                return Ok(());
            }
            "--uninstall" => return uninstall_service(),
            _ => {}
        }
    }

    // Normal service dispatch
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

windows_service::define_windows_service!(ffi_service_main, service_main);

fn service_main(_args: Vec<std::ffi::OsString>) {
    if let Err(e) = run_service() {
        error!("Service failed: {}", e);
    }
}

fn run_service() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("stopvibe_service=info")
        .with_writer(|| {
            let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
            let log_dir = std::path::PathBuf::from(base).join("StopVibe");
            std::fs::create_dir_all(&log_dir).ok();
            let log_path = log_dir.join("service.log");
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .unwrap()
        })
        .init();

    let state = Arc::new(Mutex::new(ServiceState::new()?));

    // Restore session if active
    state.lock().unwrap().restore_on_boot();

    let state_clone = state.clone();
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                // Only allow stop if no active session
                let mut svc = state_clone.lock().unwrap();
                if svc.session.as_ref().map_or(false, |s| s.is_active()) {
                    ServiceControlHandlerResult::Other(0x00000001) // reject
                } else {
                    svc.running = false;
                    ServiceControlHandlerResult::NoError
                }
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: WinServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    // Start IPC server in background thread
    let ipc_state = state.clone();
    std::thread::spawn(move || {
        if let Err(e) = ipc::run_ipc_server(ipc_state) {
            error!("IPC server error: {}", e);
        }
    });

    // Main loop: check expiry and scan processes periodically
    loop {
        std::thread::sleep(Duration::from_secs(3));

        let scan_targets = {
            let mut svc = state.lock().unwrap();
            if !svc.running {
                break;
            }
            svc.check_expiry();

            // Clone the small target list, then release the service-state lock.
            // Process enumeration can be slow, and keeping this lock during the
            // scan makes status IPC requests look disconnected under load.
            svc.session.as_ref().and_then(|session| {
                session.is_active().then(|| {
                    session
                        .targets
                        .iter()
                        .filter(|target| target.enabled)
                        .cloned()
                        .collect::<Vec<_>>()
                })
            })
        };

        if let Some(targets) = scan_targets {
            let enabled_targets: Vec<&BlockTarget> = targets.iter().collect();
            if let Err(e) = monitor::scan_and_kill(&enabled_targets) {
                error!("Process monitor scan error: {}", e);
            }
        }
    }

    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: WinServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

fn install_service() -> Result<()> {
    use std::ffi::OsString;
    use windows_service::service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType,
    };
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    let source_exe = std::env::current_exe().context("Failed to locate service executable")?;
    remove_legacy_stub_next_to(&source_exe);

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::ALL_ACCESS)?;

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from("StopVibe Blocker Service"),
        service_type: SERVICE_TYPE,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: source_exe.clone(),
        launch_arguments: vec![],
        dependencies: vec![],
        account_name: None, // LocalSystem
        account_password: None,
    };

    match manager.create_service(&service_info, ServiceAccess::ALL_ACCESS) {
        Ok(_service) => {}
        Err(create_error) => {
            manager
                .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
                .with_context(|| format!("Failed to create service: {}", create_error))?;
            configure_existing_service(&source_exe)?;
        }
    }

    let service = manager.open_service(
        SERVICE_NAME,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS,
    )?;
    let empty_args: [&str; 0] = [];
    let _ = service.start(&empty_args);
    println!("Service installed successfully.");
    Ok(())
}

fn remove_legacy_stub_next_to(service_exe: &std::path::Path) {
    let Some(install_dir) = service_exe.parent() else {
        return;
    };
    let legacy_stub = install_dir.join("stopvibe-stub.exe");
    if legacy_stub.exists() {
        if std::fs::remove_file(&legacy_stub).is_err() {
            let legacy_stub_arg = legacy_stub.to_string_lossy().to_string();
            let _ = std::process::Command::new("icacls")
                .args([legacy_stub_arg.as_str(), "/remove:d", "*S-1-1-0"])
                .output();
            let _ = std::fs::remove_file(legacy_stub);
        }
    }
}

fn uninstall_service() -> Result<()> {
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

    ensure_uninstall_allowed()?;

    stop_existing_service_for_update()?;
    delete_existing_service_best_effort()?;

    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::ALL_ACCESS)?;
    if let Ok(service) = manager.open_service(SERVICE_NAME, ServiceAccess::ALL_ACCESS) {
        let _ = service.delete();
    }

    println!("Service uninstalled successfully.");
    Ok(())
}

fn ensure_uninstall_allowed() -> Result<()> {
    let state_manager =
        StateManager::new().context("Failed to inspect StopVibe state before uninstall")?;

    if let Some(session) = state_manager.load_session()? {
        if session.is_active() {
            bail!(
                "StopVibe is still blocking. Uninstall is disabled until the active session ends in {}.",
                format_remaining(session.remaining_secs())
            );
        }
    }

    Ok(())
}

fn format_remaining(total_secs: i64) -> String {
    let total_secs = total_secs.max(0);
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

fn stop_existing_service_for_update() -> Result<()> {
    let _ = std::process::Command::new("sc")
        .args(["stop", SERVICE_NAME])
        .output();

    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(500));

        if query_service_pid()?.is_none() {
            return Ok(());
        }
    }

    bail!("StopVibeService is still running; wait for the active blocking session to finish before updating or uninstalling")
}

fn delete_existing_service_best_effort() -> Result<()> {
    let _ = std::process::Command::new("sc")
        .args(["delete", SERVICE_NAME])
        .output();

    std::thread::sleep(Duration::from_secs(1));
    Ok(())
}

fn configure_existing_service(service_exe: &std::path::Path) -> Result<()> {
    let bin_path = format!("\"{}\"", service_exe.display());
    let output = std::process::Command::new("sc")
        .args([
            "config",
            SERVICE_NAME,
            "binPath=",
            &bin_path,
            "start=",
            "auto",
        ])
        .output()
        .context("Failed to run sc config")?;

    if !output.status.success() {
        bail!(
            "Failed to configure existing service: {}",
            command_output(&output)
        );
    }

    Ok(())
}

fn query_service_pid() -> Result<Option<u32>> {
    let output = std::process::Command::new("sc")
        .args(["queryex", SERVICE_NAME])
        .output()
        .context("Failed to query service process id")?;

    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("PID") {
            let pid_text = rest.split(':').nth(1).map(str::trim).unwrap_or_default();
            if let Ok(pid) = pid_text.parse::<u32>() {
                if pid != 0 {
                    return Ok(Some(pid));
                }
            }
        }
    }

    Ok(None)
}

fn command_output(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn run_stub(original_args: &[String]) {
    use std::io::Write;
    use windows::core::PCWSTR;
    use windows::Win32::System::Console::{AttachConsole, FreeConsole, ATTACH_PARENT_PROCESS};
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};

    let target = blocked_target_from_stub_args(original_args);
    let message = blocked_message(&target);

    if target.is_cli {
        let wrote_to_console = unsafe { AttachConsole(ATTACH_PARENT_PROCESS).is_ok() }
            && std::fs::OpenOptions::new()
                .write(true)
                .open(r"\\.\CONOUT$")
                .and_then(|mut console| writeln!(console, "\n{}", message))
                .is_ok();

        unsafe {
            let _ = FreeConsole();
        }

        if wrote_to_console {
            return;
        }
    }

    let title = to_wide("StopVibe - Blocked");
    let message = to_wide(&message);

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}

struct BlockedTarget {
    name: String,
    exe_name: Option<String>,
    is_cli: bool,
}

fn blocked_target_from_stub_args(original_args: &[String]) -> BlockedTarget {
    let exe_name = original_args.first().and_then(|arg| {
        std::path::Path::new(arg)
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
    });

    let default_target = exe_name.as_ref().and_then(|exe_name| {
        stopvibe_common::default_targets()
            .into_iter()
            .find(|target| {
                target
                    .exe_names
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(exe_name))
            })
    });

    let name = default_target
        .as_ref()
        .map(|target| target.name.clone())
        .or_else(|| exe_name.clone())
        .unwrap_or_else(|| "This application".into());

    let is_cli = exe_name.as_deref().map_or(false, is_cli_exe)
        || default_target
            .as_ref()
            .map_or(false, |target| !target.cmdline_patterns.is_empty());

    BlockedTarget {
        name,
        exe_name,
        is_cli,
    }
}

fn is_cli_exe(exe_name: &str) -> bool {
    matches!(
        exe_name,
        "claude.exe" | "aider.exe" | "codex.exe" | "gemini.exe" | "goose.exe"
    )
}

fn blocked_message(target: &BlockedTarget) -> String {
    let remaining = active_remaining_text().unwrap_or_else(|| "未知".into());
    let exe_text = target
        .exe_name
        .as_ref()
        .map(|exe| format!(" ({})", exe))
        .unwrap_or_default();

    if target.is_cli {
        format!(
            "{}{} 正在被 StopVibe 禁用，剩余时间：{}。",
            target.name, exe_text, remaining
        )
    } else {
        format!(
            "{}{} 正在被 StopVibe 禁用。\n\n剩余时间：{}\n\n倒计时结束前无法解除阻止。",
            target.name, exe_text, remaining
        )
    }
}

fn active_remaining_text() -> Option<String> {
    let session = StateManager::new().ok()?.load_session().ok()??;
    if session.is_active() {
        Some(format_remaining(session.remaining_secs()))
    } else {
        None
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_targets_rejects_unknown_executables() {
        let err = sanitize_requested_targets(vec![BlockTarget {
            name: "notepad".into(),
            exe_names: vec!["notepad.exe".into()],
            cmdline_patterns: vec![],
            enabled: true,
        }])
        .unwrap_err();

        assert!(err.to_string().contains("Unknown block target"));
    }

    #[test]
    fn sanitize_targets_uses_service_owned_definitions() {
        let targets = sanitize_requested_targets(vec![BlockTarget {
            name: "OpenAI Codex CLI".into(),
            exe_names: vec!["notepad.exe".into()],
            cmdline_patterns: vec!["anything".into()],
            enabled: true,
        }])
        .unwrap();

        let codex = targets
            .iter()
            .find(|target| target.name == "OpenAI Codex CLI")
            .unwrap();
        assert!(codex.enabled);
        assert_eq!(codex.exe_names, vec!["codex.exe"]);
        assert!(!codex.cmdline_patterns.contains(&"anything".into()));
    }

    #[test]
    fn format_remaining_uses_compact_units() {
        assert_eq!(format_remaining(3661), "1h 1m 1s");
        assert_eq!(format_remaining(61), "1m 1s");
        assert_eq!(format_remaining(7), "7s");
        assert_eq!(format_remaining(-1), "0s");
    }

    #[test]
    fn stub_identifies_cli_targets() {
        let target =
            blocked_target_from_stub_args(&[r"C:\Users\me\AppData\Roaming\npm\claude.exe".into()]);

        assert_eq!(target.name, "Claude Code");
        assert_eq!(target.exe_name.as_deref(), Some("claude.exe"));
        assert!(target.is_cli);
    }

    #[test]
    fn stub_identifies_gui_targets() {
        let target = blocked_target_from_stub_args(&[
            r"C:\Users\me\AppData\Local\Programs\Cursor\Cursor.exe".into(),
        ]);

        assert_eq!(target.name, "Cursor");
        assert_eq!(target.exe_name.as_deref(), Some("cursor.exe"));
        assert!(!target.is_cli);
    }
}
