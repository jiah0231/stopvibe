use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Blocking session state persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSession {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub targets: Vec<BlockTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTarget {
    pub name: String,
    pub exe_names: Vec<String>,
    /// For Python-based CLI tools: match command-line patterns instead of exe name
    pub cmdline_patterns: Vec<String>,
    pub enabled: bool,
}

impl BlockSession {
    pub fn is_active(&self) -> bool {
        let now = Utc::now();
        now >= self.start_time && now < self.end_time
    }

    pub fn remaining_secs(&self) -> i64 {
        let remaining = self.end_time - Utc::now();
        remaining.num_seconds().max(0)
    }
}

/// IPC messages between GUI and Service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    /// Start a new blocking session
    StartBlock {
        duration_minutes: u64,
        targets: Vec<BlockTarget>,
    },
    /// Query current status
    GetStatus,
    /// Get list of available default targets
    GetDefaultTargets,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Ok,
    Status {
        active: bool,
        session: Option<BlockSession>,
    },
    DefaultTargets(Vec<BlockTarget>),
    Error(String),
}

/// Default block targets
pub fn default_targets() -> Vec<BlockTarget> {
    vec![
        BlockTarget {
            name: "Cursor".into(),
            exe_names: vec!["cursor.exe".into()],
            cmdline_patterns: vec![],
            enabled: true,
        },
        BlockTarget {
            name: "Windsurf".into(),
            exe_names: vec!["windsurf.exe".into()],
            cmdline_patterns: vec![],
            enabled: true,
        },
        BlockTarget {
            name: "VS Code (Copilot)".into(),
            exe_names: vec!["code.exe".into()],
            cmdline_patterns: vec![],
            enabled: false, // disabled by default
        },
        BlockTarget {
            name: "Claude Code".into(),
            exe_names: vec!["claude.exe".into()],
            cmdline_patterns: vec!["@anthropic-ai/claude-code".into(), "claude-code".into()],
            enabled: true,
        },
        BlockTarget {
            name: "Aider".into(),
            exe_names: vec!["aider.exe".into()],
            cmdline_patterns: vec!["aider".into(), "-m aider".into()],
            enabled: true,
        },
        BlockTarget {
            name: "OpenAI Codex CLI".into(),
            exe_names: vec!["codex.exe".into()],
            cmdline_patterns: vec!["@openai/codex".into(), "codex".into()],
            enabled: true,
        },
        BlockTarget {
            name: "Gemini CLI".into(),
            exe_names: vec!["gemini.exe".into()],
            cmdline_patterns: vec!["@google/gemini-cli".into(), "gemini-cli".into()],
            enabled: true,
        },
        BlockTarget {
            name: "Goose".into(),
            exe_names: vec!["goose.exe".into()],
            cmdline_patterns: vec![],
            enabled: true,
        },
        BlockTarget {
            name: "Kiro".into(),
            exe_names: vec!["kiro.exe".into()],
            cmdline_patterns: vec![],
            enabled: true,
        },
        BlockTarget {
            name: "Trae".into(),
            exe_names: vec!["trae.exe".into(), "trae-internal.exe".into()],
            cmdline_patterns: vec![],
            enabled: true,
        },
    ]
}

pub const SERVICE_NAME: &str = "StopVibeService";
pub const PIPE_NAME: &str = r"\\.\pipe\stopvibe-ipc";
pub const STATE_DIR: &str = "StopVibe";
pub const STATE_FILE: &str = "state.enc";
