use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

/// Lock down service files and registry during active blocking session
pub fn lock_down(state_path: &PathBuf) -> Result<()> {
    // 1. Protect service binary
    let service_exe = std::env::current_exe().context("Failed to get service exe path")?;
    deny_delete_acl(&service_exe)?;

    // 2. Protect state file
    if state_path.exists() {
        deny_delete_acl(state_path)?;
    }

    // 3. Lock service via SCM (deny stop/delete)
    lock_service_scm()?;

    // 4. Register watchdog scheduled task
    register_watchdog()?;

    info!(
        "Protection lock_down applied (state_path: {})",
        state_path.display()
    );
    Ok(())
}

/// Remove protection when session expires
pub fn unlock(state_path: &PathBuf) -> Result<()> {
    // 1. Restore service binary ACL
    let service_exe = std::env::current_exe().context("Failed to get service exe path")?;
    restore_acl(&service_exe)?;

    // 2. Restore state file ACL
    if state_path.exists() {
        restore_acl(state_path)?;
    }

    // 3. Unlock service SCM
    unlock_service_scm()?;

    // 4. Remove watchdog scheduled task
    remove_watchdog()?;

    info!("Protection unlocked (state_path: {})", state_path.display());
    Ok(())
}

/// Set NTFS ACL to deny Everyone from deleting the file
fn deny_delete_acl(path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy();
    let result = Command::new("icacls")
        .args([
            path_str.as_ref(),
            "/deny",
            "*S-1-1-0:(D)", // Everyone: deny Delete
        ])
        .output()
        .context("Failed to run icacls")?;

    if !result.status.success() {
        bail!(
            "icacls deny failed for {}: {}",
            path_str,
            command_error(&result)
        );
    }
    Ok(())
}

/// Restore default ACL (remove deny entries)
fn restore_acl(path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy();
    let result = Command::new("icacls")
        .args([
            path_str.as_ref(),
            "/remove:d",
            "*S-1-1-0", // Remove deny entries for Everyone
        ])
        .output()
        .context("Failed to run icacls")?;

    if !result.status.success() {
        bail!(
            "icacls restore failed for {}: {}",
            path_str,
            command_error(&result)
        );
    }
    Ok(())
}

/// Lock the Windows Service via SC sdset (deny stop/delete to interactive users)
fn lock_service_scm() -> Result<()> {
    // DACL that allows SYSTEM full control, denies interactive users stop/delete
    // D: DACL
    // (A;;RPWP;;;SY) - Allow SYSTEM: RP (read), WP (write)
    // (A;;CCLCSWRPWPDTLOCRRC;;;SY) - Allow SYSTEM full service control
    // (A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA) - Allow Administrators full control
    // (D;;DCLCWP;;;IU) - Deny Interactive Users: DC (stop), LC (query config), WP (change config)
    let sddl = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(D;;RPWPDTDCSD;;;IU)";

    let result = Command::new("sc")
        .args(["sdset", "StopVibeService", sddl])
        .output()
        .context("Failed to run sc sdset")?;

    if !result.status.success() {
        bail!("sc sdset lock failed: {}", command_error(&result));
    } else {
        info!("Service SCM locked");
    }
    Ok(())
}

/// Restore default service security descriptor
fn unlock_service_scm() -> Result<()> {
    // Default SDDL for a Windows service
    let sddl = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)";

    let result = Command::new("sc")
        .args(["sdset", "StopVibeService", sddl])
        .output()
        .context("Failed to run sc sdset")?;

    if !result.status.success() {
        bail!("sc sdset unlock failed: {}", command_error(&result));
    } else {
        info!("Service SCM unlocked");
    }
    Ok(())
}

/// Register a Windows Scheduled Task that acts as a watchdog
/// Checks every minute if the service is running; restarts it if not
fn register_watchdog() -> Result<()> {
    let _service_exe = std::env::current_exe()
        .context("Failed to get service exe path")?
        .to_string_lossy()
        .to_string();

    // PowerShell script that checks and restarts the service
    let ps_script = r#"$svc = Get-Service -Name 'StopVibeService' -ErrorAction SilentlyContinue; if ($svc -and $svc.Status -ne 'Running') { Start-Service -Name 'StopVibeService' }"#;

    let result = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            "StopVibeWatchdog",
            "/SC",
            "MINUTE",
            "/MO",
            "1",
            "/TR",
            &format!(
                "powershell.exe -NoProfile -WindowStyle Hidden -Command \"{}\"",
                ps_script
            ),
            "/RU",
            "SYSTEM",
            "/RL",
            "HIGHEST",
            "/F", // Force overwrite if exists
        ])
        .output()
        .context("Failed to create watchdog task")?;

    if !result.status.success() {
        bail!("Failed to register watchdog: {}", command_error(&result));
    } else {
        info!("Watchdog scheduled task registered");
    }
    Ok(())
}

/// Remove the watchdog scheduled task
fn remove_watchdog() -> Result<()> {
    let result = Command::new("schtasks")
        .args(["/Delete", "/TN", "StopVibeWatchdog", "/F"])
        .output()
        .context("Failed to delete watchdog task")?;

    if !result.status.success() {
        let message = command_error(&result);
        if !task_missing_message(&message) {
            bail!("Failed to remove watchdog: {}", message);
        }
    } else {
        info!("Watchdog scheduled task removed");
    }
    Ok(())
}

fn command_error(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn task_missing_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("does not exist")
        || message.contains("cannot find")
        || message.contains("the system cannot find")
        || message.contains("找不到")
}
