//! `/api/v1/system` endpoints.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use watchgrid_model::{CameraStatus, LogEntry, LogLevel, LogQuery, ServerHealth, ServerInfo, SystemStatus};

use super::health;

use crate::cameras;
use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/info", get(info))
        .route("/status", get(status))
        .route("/logs", get(logs))
        .route("/capacity", get(crate::capacity::handler))
}

async fn info(State(s): State<AppState>) -> ApiResult<Json<ServerInfo>> {
    let name = crate::settings::load_app(&s.db, s.bind).await?.general.nvr_name;
    Ok(Json(ServerInfo { name, version: env!("CARGO_PKG_VERSION").into(),
        started_at: s.started_at,
        log_level_from_env: crate::settings::applied::log_level_from_env(),
    }))
}

async fn status(State(s): State<AppState>) -> ApiResult<Json<SystemStatus>> {
    let host = s.metrics.latest();
    // Counts only: no storage totals or event lookups on this 5 s poll.
    let cams = cameras::list_live(&s).await?;
    let enabled = cams.iter().filter(|c| c.enabled);
    let root = s.recording_files.root();
    let disk = crate::storage::disk_usage_percent(&root);
    let targets = crate::exports::repo::targets(&s.db).await?;
    let failing: Vec<String> = targets.iter().filter(|t| t.problem.is_some()).map(|t| t.name.clone()).collect();
    let mut checks = vec![health::database(), health::recordings(&root, disk), health::cameras(&cams)];
    checks.extend(health::motion(&cams));
    checks.extend(health::exports(&failing, targets.len()));
    Ok(Json(SystemStatus {
        health: ServerHealth::of(&checks),
        checks,
        uptime: (chrono::Utc::now() - s.started_at).num_seconds().max(0) as u64,
        cpu_usage: host.cpu,
        memory_used: host.memory_used,
        memory_total: host.memory_total,
        process_memory: host.process_memory,
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
