use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::thread;
use std::time::Duration;
use stopvibe_common::{IpcRequest, IpcResponse, PIPE_NAME};
use windows::core::PCSTR;
use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows::Win32::Storage::FileSystem::{
    CreateFileA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE, OPEN_EXISTING,
};

pub fn send_request(request: &IpcRequest) -> Result<IpcResponse> {
    let file = connect_pipe_with_retry()?;
    let mut writer = file.try_clone()?;
    let mut reader = BufReader::new(file);

    let request_json = serde_json::to_string(request)?;
    writer.write_all(request_json.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;

    let mut response_line = String::new();
    reader.read_line(&mut response_line)?;

    let response: IpcResponse = serde_json::from_str(response_line.trim())?;
    Ok(response)
}

fn connect_pipe_with_retry() -> Result<std::fs::File> {
    let pipe_name = format!("{}\0", PIPE_NAME);
    let mut last_error = None;

    for attempt in 0..25 {
        match unsafe {
            CreateFileA(
                PCSTR(pipe_name.as_ptr()),
                0x80000000 | 0x40000000, // GENERIC_READ | GENERIC_WRITE
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        } {
            Ok(handle) if handle != INVALID_HANDLE_VALUE => {
                let file: std::fs::File = unsafe { FromRawHandle::from_raw_handle(handle.0 as _) };
                return Ok(file);
            }
            Ok(_) => {
                last_error = Some(anyhow::anyhow!("Pipe returned an invalid handle"));
            }
            Err(e) => {
                last_error = Some(e.into());
            }
        }

        thread::sleep(Duration::from_millis(40 + attempt * 10));
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Timed out connecting to service pipe")))
        .context("Failed to connect to service pipe")
}
