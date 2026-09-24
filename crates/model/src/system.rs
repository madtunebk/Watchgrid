//! Server identity, health, resource usage and logs.

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerHealth {
    Running,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub health: ServerHealth,
    pub started_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    /// Seconds
    pub uptime: u64,
    /// 0-100
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    /// Bytes/s
    pub network_rx: u64,
    pub network_tx: u64,
    pub active_streams: u32,
    pub active_recordings: u32,
    pub connected_cameras: u32,
    pub total_cameras: u32,
    /// 0-100
    pub disk_usage: f32,
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
