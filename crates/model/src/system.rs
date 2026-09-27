//! Server identity, health, resource usage and logs.

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

/// The NVR as a whole, from its [`HealthCheck`]s: the worst one decides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerHealth {
    /// Every check passes.
    #[default]
    Running,
    /// Works, but something needs attention (a camera offline, a disk
    /// almost full, an export destination failing).
    Degraded,
    /// Something essential fails (new recordings can't be written).
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckLevel {
    Ok,
    Warning,
    Error,
}

/// One part of the NVR and how it is doing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    /// "Database", "Recordings folder", "Cameras", "Export destinations".
    pub name: String,
    pub level: CheckLevel,
    /// Short and plain: "Writable, 41% used", "HallWay offline".
    pub detail: String,
}

impl ServerHealth {
    pub fn of(checks: &[HealthCheck]) -> Self {
        match checks.iter().map(|c| c.level).max() {
            Some(CheckLevel::Error) => Self::Stopped,
            Some(CheckLevel::Warning) => Self::Degraded,
            _ => Self::Running,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub started_at: Timestamp,
    /// `RUST_LOG` is set on the server, so Settings → Advanced → Log level
    /// has no effect.
    #[serde(default)]
    pub log_level_from_env: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    /// Seconds
    pub uptime: u64,
    /// 0-100
    pub cpu_usage: f32,
    /// Host memory in use / installed (the whole machine).
    pub memory_used: u64,
    pub memory_total: u64,
    /// Memory used by the Watchgrid process itself (resident set).
    #[serde(default)]
    pub process_memory: u64,
    /// Bytes/s
    pub network_rx: u64,
    pub network_tx: u64,
    pub active_streams: u32,
    pub active_recordings: u32,
    pub connected_cameras: u32,
    pub total_cameras: u32,
    /// 0-100
    pub disk_usage: f32,
    /// Worst of `checks`.
    #[serde(default)]
    pub health: ServerHealth,
    #[serde(default)]
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: Id,
    pub time: Timestamp,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
}

/// Filter for `GET /system/logs`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    /// Only this level and above.
    pub min_level: Option<LogLevel>,
    /// Case-insensitive text match on source or message.
    pub search: Option<String>,
    /// Newest entries, at most this many.
    pub limit: Option<u32>,
}
