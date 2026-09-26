//! Mock server info and live system metrics.

use chrono::Utc;

use super::db::with_db;
use super::sim::{jitter, latency};
use crate::api::{ApiResult, CameraStatus, ServerInfo, SystemStatus};

pub async fn server_info() -> ApiResult<ServerInfo> {
    latency().await;
    Ok(with_db(|db| db.server.clone()))
}

pub async fn status() -> ApiResult<SystemStatus> {
    latency().await;
    const MB: f64 = 1024.0 * 1024.0;
    Ok(with_db(|db| {
        let online = db.cameras.iter().filter(|c| c.enabled && c.status == CameraStatus::Online).count() as u32;
        let recording = db.cameras.iter().filter(|c| c.recording_active).count() as u32;
        SystemStatus {
            uptime: (Utc::now() - db.server.started_at).num_seconds().max(0) as u64,
            cpu_usage: jitter(6.0 + online as f64 * 2.5, 6.0).max(1.0) as f32,
            memory_used: (jitter(180.0 + online as f64 * 55.0, 20.0) * MB) as u64,
            memory_total: (2048.0 * MB) as u64,
            process_memory: (38.0 * MB) as u64,
            network_rx: (jitter(0.55 * online as f64, 0.3).max(0.0) * MB) as u64,
            network_tx: (jitter(0.08 * online as f64, 0.05).max(0.0) * MB) as u64,
            active_streams: online,
            active_recordings: recording,
            connected_cameras: online,
            total_cameras: db.cameras.len() as u32,
            disk_usage: 41.0,
        }
    }))
}
