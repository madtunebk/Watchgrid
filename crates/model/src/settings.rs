//! Server-wide settings. Every section applies live; none needs a restart
//! except the HTTP bind address/port, which the UI warns about.

use serde::{Deserialize, Serialize};

use crate::{LogLevel, RecordingMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    /// 2026-09-24
    Iso,
    /// 24/09/2026
    DayFirst,
    /// 09/24/2026
    MonthFirst,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub nvr_name: String,
    /// IANA zone, e.g. "Europe/Bucharest".
    pub timezone: String,
    /// BCP-47, e.g. "en", "ro".
    pub language: String,
    pub date_format: DateFormat,
    pub clock_24h: bool,
}

/// Defaults applied to newly added cameras.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingDefaults {
    pub mode: RecordingMode,
    pub pre_record_seconds: u32,
    pub post_record_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSettings {
    pub http_bind: String,
    pub http_port: u16,
    /// Future: HTTPS with a provided or self-signed certificate.
    pub https_enabled: bool,
    pub https_port: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSettings {
    pub enabled: bool,
    pub session_timeout_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    pub camera_offline: bool,
    pub person_detected: bool,
    pub vehicle_detected: bool,
    pub storage_low: bool,
    pub recording_failed: bool,
    /// A camera reported a sign-in with a wrong password.
    #[serde(default = "yes")]
    pub camera_security: bool,
    /// POST a JSON payload here for every notification.
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RtspTransport {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedSettings {
    pub log_level: LogLevel,
    pub rtsp_transport: RtspTransport,
    pub reconnect_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub general: GeneralSettings,
    pub recording: RecordingDefaults,
    pub network: NetworkSettings,
    pub auth: AuthSettings,
    pub notifications: NotificationSettings,
    pub advanced: AdvancedSettings,
}

fn yes() -> bool {
    true
}
