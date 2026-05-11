use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use stopvibe_common::BlockTarget;
use tracing::info;
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_CREATE_KEY_DISPOSITION,
    REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE,
};

const IFEO_BASE: &str =
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
const DEBUGGER_VALUE_NAME: &str = "Debugger";

struct RegistryKey(HKEY);

impl RegistryKey {
    fn raw(&self) -> HKEY {
        self.0
    }
}

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0).ok();
        }
    }
}

fn get_debugger_value() -> Result<String> {
    let service_exe = std::env::current_exe().context("Failed to get service executable path")?;
    Ok(format!("\"{}\" --stub", service_exe.to_string_lossy()))
}

pub fn apply_ifeo_blocks(targets: &[&BlockTarget]) -> Result<()> {
    let debugger_value = get_debugger_value()?;
    let owned_debugger_values = get_owned_debugger_values()?;

    for target in targets {
        for exe_name in &target.exe_names {
            set_ifeo_debugger(exe_name, &debugger_value, &owned_debugger_values)
                .with_context(|| format!("Failed to set IFEO for {}", exe_name))?;
            info!("IFEO block applied for {}", exe_name);
        }
    }
    Ok(())
}

pub fn remove_ifeo_blocks(targets: &[&BlockTarget]) -> Result<()> {
    let debugger_values = get_owned_debugger_values()?;

    for target in targets {
        for exe_name in &target.exe_names {
            remove_ifeo_debugger(exe_name, &debugger_values)
                .with_context(|| format!("Failed to remove IFEO for {}", exe_name))?;
            info!("IFEO block removed for {}", exe_name);
        }
    }
    Ok(())
}

fn get_owned_debugger_values() -> Result<Vec<String>> {
    let current_exe = std::env::current_exe().context("Failed to get service executable path")?;
    let mut values = vec![format!("\"{}\" --stub", current_exe.to_string_lossy())];

    if let Some(exe_dir) = current_exe.parent() {
        push_legacy_stub_values(&mut values, exe_dir.join("stopvibe-stub.exe"));
    }

    if let Ok(program_files) = std::env::var("ProgramFiles") {
        push_legacy_stub_values(
            &mut values,
            PathBuf::from(program_files)
                .join("StopVibe")
                .join("stopvibe-stub.exe"),
        );
    }

    values.sort_by_key(|value| normalize_debugger(value));
    values.dedup_by(|a, b| normalize_debugger(a) == normalize_debugger(b));
    Ok(values)
}

fn push_legacy_stub_values(values: &mut Vec<String>, stub_exe: PathBuf) {
    let stub_exe = stub_exe.display();
    values.push(format!("\"{}\"", stub_exe));
    values.push(format!("\"{}\" --stub", stub_exe));
}

fn set_ifeo_debugger(
    exe_name: &str,
    debugger_value: &str,
    owned_debugger_values: &[String],
) -> Result<()> {
    let subkey = format!("{}\\{}", IFEO_BASE, exe_name);
    let subkey_wide = to_wide(&subkey);

    let mut hkey = HKEY::default();
    let mut disposition = REG_CREATE_KEY_DISPOSITION::default();

    let key = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            None,
            &mut hkey,
            Some(&mut disposition),
        )
        .ok()
        .context("RegCreateKeyExW failed")?;
        RegistryKey(hkey)
    };

    if let Some(existing) = query_debugger_value(key.raw())? {
        if !owned_debugger_values
            .iter()
            .any(|owned| debugger_value_matches(&existing, owned))
        {
            bail!(
                "IFEO Debugger for {} is already set to another value",
                exe_name
            );
        }
    }

    unsafe {
        let value_name = to_wide(DEBUGGER_VALUE_NAME);
        let value_data = to_wide(debugger_value);
        let data_bytes: &[u8] =
            std::slice::from_raw_parts(value_data.as_ptr() as *const u8, value_data.len() * 2);

        RegSetValueExW(
            hkey,
            PCWSTR(value_name.as_ptr()),
            0,
            REG_SZ,
            Some(data_bytes),
        )
        .ok()
        .context("RegSetValueExW failed")?;
    }

    Ok(())
}

fn remove_ifeo_debugger(exe_name: &str, expected_debuggers: &[String]) -> Result<()> {
    let subkey = format!("{}\\{}", IFEO_BASE, exe_name);
    let subkey_wide = to_wide(&subkey);

    let mut hkey = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(subkey_wide.as_ptr()),
            0,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            &mut hkey,
        )
    };

    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status.ok().context("RegOpenKeyExW failed")?;
    let key = RegistryKey(hkey);

    let Some(existing) = query_debugger_value(key.raw())? else {
        return Ok(());
    };

    if !expected_debuggers
        .iter()
        .any(|expected| debugger_value_matches(&existing, expected))
    {
        bail!(
            "Refusing to remove IFEO Debugger for {} because it is not owned by StopVibe",
            exe_name
        );
    }

    unsafe {
        let value_name = to_wide(DEBUGGER_VALUE_NAME);
        RegDeleteValueW(key.raw(), PCWSTR(value_name.as_ptr()))
            .ok()
            .context("RegDeleteValueW failed")?;
    }

    Ok(())
}

fn query_debugger_value(hkey: HKEY) -> Result<Option<String>> {
    let value_name = to_wide(DEBUGGER_VALUE_NAME);
    let mut value_type = REG_VALUE_TYPE::default();
    let mut data_len = 0u32;

    let status = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            None,
            Some(&mut data_len),
        )
    };

    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok().context("RegQueryValueExW size query failed")?;

    if value_type != REG_SZ {
        bail!("Existing IFEO Debugger value is not REG_SZ");
    }

    if data_len == 0 {
        return Ok(Some(String::new()));
    }

    let mut data = vec![0u8; data_len as usize];
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            Some(data.as_mut_ptr()),
            Some(&mut data_len),
        )
    };
    status.ok().context("RegQueryValueExW value query failed")?;
    data.truncate(data_len as usize);

    let mut words: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    while words.last() == Some(&0) {
        words.pop();
    }

    Ok(Some(String::from_utf16_lossy(&words)))
}

fn debugger_value_matches(existing: &str, expected: &str) -> bool {
    normalize_debugger(existing) == normalize_debugger(expected)
}

fn normalize_debugger(value: &str) -> String {
    value.trim().trim_matches('"').to_ascii_lowercase()
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
