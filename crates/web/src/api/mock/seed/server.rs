use chrono::{Duration, Utc};

use crate::api::{ServerHealth, ServerInfo};

pub fn info() -> ServerInfo {
    ServerInfo {
        name: "Home NVR".into(),
        version: "0.1.0-dev".into(),
        health: ServerHealth::Running,
        started_at: Utc::now() - Duration::seconds(3 * 86_400 + 5 * 3_600 + 17 * 60),
        log_level_from_env: false,
    }
}
