# StopVibe

**StopVibe** is a Windows focus app that blocks AI coding tools during a timed focus session.

Set a timer, choose the tools you want to block, and StopVibe keeps them unavailable until the timer ends. It is meant for people who enjoy AI coding tools but sometimes need a firm boundary to think, read, debug, or write code on their own.

**Download:** [GitHub Releases](https://github.com/jiah0231/stopvibe/releases)

## Background

AI coding tools are incredibly useful, but they can also make it too easy to skip the uncomfortable parts of programming: sitting with a problem, reading unfamiliar code, debugging patiently, and forming your own mental model.

StopVibe was built for that specific tension. It is not anti-AI. It is a small self-control tool for people who like AI coding assistants but sometimes want a real, enforceable break from them. Instead of relying on willpower in the moment, you decide in advance: for the next block of time, these tools are off-limits.

The goal is not to punish yourself. The goal is to create enough quiet space for deliberate practice, deeper focus, and more independent thinking.

## What It Blocks

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

## Features

- Timed focus sessions for AI coding tools.
- A simple Windows desktop UI.
- Background Windows service enforcement.
- Launch blocking for selected apps.
- Running-process monitor for tools already open.
- Session state survives app restarts and reboots.
- Normal uninstall is blocked while a timer is active.
- No need to run the UI as administrator after installation.

## Installation

Download the latest installer from [Releases](https://github.com/jiah0231/stopvibe/releases).

Recommended:

- `StopVibe_0.1.0_x64-setup.exe`

Alternative:

- `StopVibe_0.1.0_x64_en-US.msi`

The installer needs one Windows UAC confirmation because StopVibe registers a background service. After that, open StopVibe normally from the Start menu or desktop shortcut.

## Usage

1. Open StopVibe.
2. Select the tools you want to block.
3. Choose a duration.
4. Start the session.

During an active session, blocked tools cannot be launched normally. If one is already running, the service will close it.

## Important Notes

StopVibe is a focus tool, not security software. A determined Windows administrator can still remove services, edit the registry, delete files, or boot into recovery/safe mode. StopVibe is designed to stop normal, impulsive bypasses, not to defeat the owner of the machine.

The timer currently uses wall-clock time. If you start a one-hour session and keep the computer powered off for an hour, the session may expire while the machine is off.

## For Developers

Tech stack:

- Rust
- Tauri
- React
- Windows Service APIs

Project layout:

```text
crates/stopvibe-common   Shared IPC types and target definitions
crates/stopvibe-service  Windows service, blocker, monitor, state, protection
src-tauri                Tauri desktop shell and installer hooks
ui                       React frontend
scripts                  Build helper scripts
```

## License

MIT

---

# StopVibe 中文说明

**StopVibe** 是一个 Windows 专注工具，用来在倒计时期间阻止 AI 编程工具。

你设置一个时间，选择要屏蔽的软件，然后 StopVibe 会在倒计时结束前持续阻止这些工具打开。它适合那种“我喜欢 AI 编程工具，但现在想靠自己思考、读代码、调 bug、写代码”的时刻。

**下载地址：** [GitHub Releases](https://github.com/jiah0231/stopvibe/releases)

## 项目背景

AI 编程工具非常有用，但它也很容易让人跳过编程里最难、也最值得练习的部分：和问题待在一起，读不熟悉的代码，耐心调试，自己建立对系统的理解。

StopVibe 就是为这种矛盾做的。它不是反 AI，也不是否定 AI 编程工具，而是给喜欢这些工具的人一个“可以认真停下来”的硬边界。你不需要在冲动的时候和自己拉扯，而是在开始前先决定：接下来这段时间，这些工具先不能用。

它的目标不是惩罚自己，而是给独立思考、刻意练习和深度专注留出一段安静的空间。

## 默认阻止的软件

默认启用：

- Cursor
- Windsurf
- Claude Code / Claude CLI
- Aider
- OpenAI Codex CLI
- Gemini CLI
- Goose
- Kiro
- Trae

默认提供但不启用：

- VS Code

## 功能

- 给 AI 编程工具设置专注倒计时。
- 简洁的 Windows 桌面界面。
- 后台 Windows 服务负责执行阻止。
- 阻止所选软件启动。
- 自动关闭已经在运行的被阻止工具。
- 重启应用或重启电脑后仍能恢复未结束的会话。
- 倒计时未结束时，正常卸载流程会被拒绝。
- 安装后普通打开即可，不需要每次右键“以管理员身份运行”。

## 安装

到 [Releases](https://github.com/jiah0231/stopvibe/releases) 下载最新版。

推荐下载：

- `StopVibe_0.1.0_x64-setup.exe`

也可以下载：

- `StopVibe_0.1.0_x64_en-US.msi`

安装时会弹一次 Windows UAC 权限确认，因为 StopVibe 需要注册后台服务。安装完成后，直接从开始菜单或桌面快捷方式普通打开即可。

## 使用

1. 打开 StopVibe。
2. 勾选要阻止的工具。
3. 设置倒计时时长。
4. 开始专注。

倒计时期间，被阻止的软件无法正常启动。如果它已经在运行，后台服务也会自动关闭它。

## 重要说明

StopVibe 是专注辅助工具，不是安全软件。真正拥有管理员权限的人仍然可以删除服务、改注册表、删文件，或者进安全模式清理。StopVibe 的目标是挡住日常使用里的冲动绕过，而不是对抗电脑所有者。

当前计时方式是按真实时间计算。如果你开启 1 小时专注，然后关机 1 小时，回来后这次专注可能已经过期。

## 开发者

技术栈：

- Rust
- Tauri
- React
- Windows Service APIs

项目结构：

```text
crates/stopvibe-common   共享 IPC 类型和默认目标定义
crates/stopvibe-service  Windows 服务、阻止逻辑、进程监控、状态和保护
src-tauri                Tauri 桌面壳和安装器钩子
ui                       React 前端
scripts                  构建辅助脚本
```

## 许可证

MIT
