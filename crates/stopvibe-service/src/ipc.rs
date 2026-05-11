use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::time::Duration;
use stopvibe_common::{IpcRequest, IpcResponse, PIPE_NAME};
use tracing::{error, info};
use windows::core::{PCSTR, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, BOOL, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL,
    INVALID_HANDLE_VALUE,
};
use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeA, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

use crate::ServiceState;

const BUFFER_SIZE: u32 = 4096;
const SECURITY_DESCRIPTOR_REVISION: u32 = 1;

struct PipeSecurity {
    descriptor: PSECURITY_DESCRIPTOR,
    attributes: SECURITY_ATTRIBUTES,
}

impl PipeSecurity {
    fn new() -> Result<Self> {
        // SYSTEM and Administrators get full access; interactive users can talk to
        // the service, but the service still validates every requested target.
        let sddl = to_wide("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)");
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SECURITY_DESCRIPTOR_REVISION,
                &mut descriptor,
                None,
            )
            .context("Failed to build named pipe security descriptor")?;
        }

        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: BOOL(0),
        };

        Ok(Self {
            descriptor,
            attributes,
        })
    }

    fn attributes_ptr(&self) -> *const SECURITY_ATTRIBUTES {
        &self.attributes
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        if !self.descriptor.is_invalid() {
            unsafe {
                let _ = LocalFree(HLOCAL(self.descriptor.0));
            }
        }
    }
}

pub fn run_ipc_server(state: std::sync::Arc<std::sync::Mutex<ServiceState>>) -> Result<()> {
    let pipe_name = format!("{}\0", PIPE_NAME);
    let pipe_security = PipeSecurity::new()?;

    loop {
        let handle = unsafe {
            CreateNamedPipeA(
                PCSTR(pipe_name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER_SIZE,
                BUFFER_SIZE,
                0,
                Some(pipe_security.attributes_ptr()),
            )
        }?;

        if handle == INVALID_HANDLE_VALUE {
            error!("Failed to create named pipe");
            continue;
        }

        let connected = unsafe { ConnectNamedPipe(handle, None) };
        if connected.is_err() {
            let last_error = unsafe { GetLastError() };
            if last_error != ERROR_PIPE_CONNECTED {
                error!("IPC pipe connect failed: {}", last_error.0);
                unsafe { CloseHandle(handle) }.ok();
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
        }

        info!("IPC client connected");

        let raw_handle = handle.0 as isize;
        let client_state = state.clone();
        std::thread::spawn(move || {
            let handle = HANDLE(raw_handle as _);
            if let Err(e) = handle_client(handle, &client_state) {
                error!("IPC client error: {}", e);
            }
        });
    }
}

fn handle_client(
    handle: HANDLE,
    state: &std::sync::Arc<std::sync::Mutex<ServiceState>>,
) -> Result<()> {
    let file: std::fs::File = unsafe { FromRawHandle::from_raw_handle(handle.0 as _) };
    let mut reader = BufReader::new(file.try_clone()?);
    let mut writer = file;

    let mut line = String::new();
    reader.read_line(&mut line)?;

    let request: IpcRequest = serde_json::from_str(line.trim())?;
    let response = process_request(request, state);

    let response_json = serde_json::to_string(&response)?;
    writer.write_all(response_json.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    Ok(())
}

fn process_request(
    request: IpcRequest,
    state: &std::sync::Arc<std::sync::Mutex<ServiceState>>,
) -> IpcResponse {
    let mut svc = state.lock().unwrap();

    match request {
        IpcRequest::StartBlock {
            duration_minutes,
            targets,
        } => match svc.start_blocking(duration_minutes, targets) {
            Ok(()) => IpcResponse::Ok,
            Err(e) => IpcResponse::Error(e.to_string()),
        },
        IpcRequest::GetStatus => {
            let session = svc.current_session();
            let active = session.as_ref().map_or(false, |s| s.is_active());
            IpcResponse::Status { active, session }
        }
        IpcRequest::GetDefaultTargets => {
            IpcResponse::DefaultTargets(stopvibe_common::default_targets())
        }
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
