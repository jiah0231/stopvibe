# StopVibe

StopVibe is a Windows focus tool that blocks AI coding clients for a fixed session. It is designed for moments when you want a hard boundary: start a timer, pick the tools to block, and let the background service enforce the block until the timer ends.

> This project is intentionally strict. During an active session, normal app controls and normal uninstall flows refuse to remove the block.

## Features

- Windows desktop UI built with Tauri + React.
- Background Windows service that runs independently of the UI.
- IFEO launch blocking for selected executables.
- Process monitor that terminates already-running blocked tools.
- Encrypted session state in `C:\ProgramData\StopVibe\state.enc`.
- Auto-start service restores active sessions after reboot.
- Self-protection for service files, state file, service permissions, and watchdog task.
- Active timer prevents normal NSIS/MSI uninstall until the session expires.
- No separate blocker stub executable: blocked-app popup is handled by `stopvibe-service.exe --stub`.

## Default Targets

Enabled by default:

- Cursor
- Windsurf
- Claude Code / Claude CLI
- Aider
- OpenAI Codex CLI
- Gemini CLI
- Goose
- Kiro
- Trae

Available but disabled by default:

- VS Code

## How It Works

StopVibe has two main executables:

- `stopvibe-tauri.exe`: the desktop control panel.
- `stopvibe-service.exe`: the Windows service and blocked-app popup handler.

When a session starts, the service writes IFEO `Debugger` values for selected targets. Those values point to:

```text
stopvibe-service.exe --stub
```

If a blocked app is launched, Windows starts the service executable in stub mode and shows a blocked message. The service also scans running processes every few seconds and terminates matching tools.

## Limitations

StopVibe is a focus aid, not malware or an anti-tamper rootkit. A determined administrator can still remove Windows services, edit the registry, delete files, or boot into recovery/safe mode. The goal is to block normal workflows and reduce impulsive bypasses, not to defeat the operating system owner.

The timer currently uses wall-clock time. If a one-hour session is started and the computer is powered off for an hour, the session can expire while the machine is off.

## Requirements

- Windows 10/11
- Rust toolchain
- Node.js and npm
- Tauri build prerequisites
- WiX Toolset / NSIS when building installer bundles through Tauri

## Build

Install frontend dependencies:

```powershell
cd ui
npm install
cd ..
```

Build installer bundles:

```powershell
cargo tauri build
```

This runs `scripts\prepare-tauri-resources.ps1`, which builds the service and copies it into the Tauri resource folder before packaging.

Build Rust crates only:

```powershell
cargo build --release
```

## Install

The recommended path is to build and run the generated installer from:

```text
target\release\bundle\nsis\
target\release\bundle\msi\
```

The installer runs per-machine and prompts for UAC once so it can register the service. After installation, the UI can be opened normally without right-clicking "Run as administrator".

For development installs, you can also run:

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

## Uninstall

Use Windows Settings, the generated uninstaller, or:

```powershell
powershell -ExecutionPolicy Bypass -File .\uninstall.ps1
```

If a blocking session is active, normal uninstall is refused until the timer expires.

## Project Layout

```text
crates/stopvibe-common   Shared IPC types and target definitions
crates/stopvibe-service  Windows service, IFEO blocker, monitor, state, protection
src-tauri                Tauri desktop shell and installer hooks
ui                       React frontend
scripts                  Build helper scripts
```

## License

MIT
