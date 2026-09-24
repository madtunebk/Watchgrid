//! `/api/v1/system` endpoints.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use watchgrid_model::{CameraStatus, LogEntry, LogLevel, LogQuery, ServerHealth, ServerInfo, SystemStatus};

use crate::cameras;
use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/info", get(info)).route("/status", get(status)).route("/logs", get(logs))
}

async fn info(State(s): State<AppState>) -> Json<ServerInfo> {
    Json(ServerInfo { name: "Watchgrid".into(), version: env!("CARGO_PKG_VERSION").into(), health: ServerHealth::Running, started_at: s.started_at })
}

async fn status(State(s): State<AppState>) -> ApiResult<Json<SystemStatus>> {
    let host = s.metrics.latest();
    let cams = cameras::list(&s).await?;
    let enabled = cams.iter().filter(|c| c.enabled);
    let disk = crate::storage::disk_usage_percent(s.recording_files.root());
    Ok(Json(SystemStatus {
        uptime: (chrono::Utc::now() - s.started_at).num_seconds().max(0) as u64,
        cpu_usage: host.cpu,
        memory_used: host.memory_used,
        memory_total: host.memory_total,
        network_rx: host.rx,
        network_tx: host.tx,
        active_streams: s.media.active_feeds() as u32,
        active_recordings: cams.iter().filter(|c| c.recording_active).count() as u32,
        connected_cameras: enabled.filter(|c| c.status == CameraStatus::Online).count() as u32,
        total_cameras: cams.len() as u32,
        disk_usage: disk.unwrap_or(0.0),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogParams {
    min_level: Option<LogLevel>,
    search: Option<String>,
    limit: Option<u32>,
}

async fn logs(State(s): State<AppState>, Query(p): Query<LogParams>) -> Json<Vec<LogEntry>> {
    Json(s.logs.query(&LogQuery { min_level: p.min_level, search: p.search, limit: p.limit }))
}
