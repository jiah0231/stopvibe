mod ipc;

use ipc::send_request;
use stopvibe_common::{BlockTarget, IpcRequest, IpcResponse};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

#[tauri::command]
async fn get_status() -> Result<serde_json::Value, String> {
    match send_request(&IpcRequest::GetStatus) {
        Ok(IpcResponse::Status { active, session }) => {
            Ok(serde_json::json!({ "active": active, "session": session }))
        }
        Ok(IpcResponse::Error(e)) => Err(e),
        Err(e) => Err(format!("Service connection failed: {}", e)),
        _ => Err("Unexpected response".into()),
    }
}

#[tauri::command]
async fn get_default_targets() -> Result<Vec<BlockTarget>, String> {
    match send_request(&IpcRequest::GetDefaultTargets) {
        Ok(IpcResponse::DefaultTargets(targets)) => Ok(targets),
        Ok(IpcResponse::Error(e)) => Err(e),
        Err(e) => Err(format!("Service connection failed: {}", e)),
        _ => Err("Unexpected response".into()),
    }
}

#[tauri::command]
async fn start_block(duration_minutes: u64, targets: Vec<BlockTarget>) -> Result<(), String> {
    let request = IpcRequest::StartBlock {
        duration_minutes,
        targets,
    };
    match send_request(&request) {
        Ok(IpcResponse::Ok) => Ok(()),
        Ok(IpcResponse::Error(e)) => Err(e),
        Err(e) => Err(format!("Service connection failed: {}", e)),
        _ => Err("Unexpected response".into()),
    }
}

#[tauri::command]
async fn install_service() -> Result<String, String> {
    // Find the bundled service first; same-directory services may be stale from
    // an older install and are repaired by the bundled service's --install path.
    let exe_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .unwrap()
        .to_path_buf();

    let service_exe = find_service_exe(&exe_dir)?;

    run_elevated_and_wait(&service_exe, "--install")?;

    for _ in 0..20 {
        if send_request(&IpcRequest::GetStatus).is_ok() {
            return Ok("Service installed and running".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    Err("Service install completed, but the IPC endpoint did not respond".into())
}

fn find_service_exe(exe_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    // Check resources subdirectory (Tauri bundles resources here on Windows)
    let candidate = exe_dir.join("resources").join("stopvibe-service.exe");
    if candidate.exists() {
        return Ok(candidate);
    }
    // Check same directory next
    let candidate = exe_dir.join("stopvibe-service.exe");
    if candidate.exists() {
        return Ok(candidate);
    }
    // Check parent directory (dev scenario: target/release/)
    if let Some(parent) = exe_dir.parent() {
        let candidate = parent.join("stopvibe-service.exe");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "stopvibe-service.exe not found near {}",
        exe_dir.display()
    ))
}

fn run_elevated_and_wait(exe: &std::path::Path, args: &str) -> Result<(), String> {
    let exe_wide = to_wide(&exe.to_string_lossy());
    let args_wide = to_wide(args);
    let verb_wide = to_wide("runas");

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: HWND::default(),
        lpVerb: windows::core::PCWSTR(verb_wide.as_ptr()),
        lpFile: windows::core::PCWSTR(exe_wide.as_ptr()),
        lpParameters: windows::core::PCWSTR(args_wide.as_ptr()),
        lpDirectory: windows::core::PCWSTR::null(),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| format!("UAC elevation failed: {}", e))?;
        if !info.hProcess.is_invalid() {
            WaitForSingleObject(info.hProcess, 60_000);
            CloseHandle(info.hProcess).ok();
        }
    }

    Ok(())
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_default_targets,
            start_block,
            install_service,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
