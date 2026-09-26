//! `/api/v1/storage` endpoints.

use axum::extract::State;
use axum::routing::{get, put};
use axum::{Json, Router};
use watchgrid_model::{RetentionPolicy, StorageStatus};

use super::{disk, location, retention};
use crate::bus::BusEvent;
use crate::error::ApiResult;
use crate::recordings;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(status)).route("/retention", put(update_retention)).route("/path", put(update_path))
}

async fn status(State(s): State<AppState>) -> ApiResult<Json<StorageStatus>> {
    let root = s.recording_files.root();
    let path = std::fs::canonicalize(&root).unwrap_or(root);
    let space = disk::space(&path);
    if let Err(e) = &space {
        tracing::warn!("cannot read disk space of {}: {e}", path.display());
    }
    let per_camera = recordings::usage_by_camera(&s.db).await?;
    let space = space.ok();
    Ok(Json(StorageStatus {
        path: path.display().to_string(),
        available: space.is_some(),
        total: space.map_or(0, |d| d.total),
        used: space.map_or(0, |d| d.used),
        free: space.map_or(0, |d| d.free),
        recordings_size: per_camera.iter().map(|u| u.bytes).sum(),
        protected_size: recordings::protected_bytes(&s.db).await?,
        per_camera,
        retention: retention::load(&s.db).await?,
    }))
}

async fn update_retention(State(s): State<AppState>, Json(policy): Json<RetentionPolicy>) -> ApiResult<Json<RetentionPolicy>> {
    retention::save(&s.db, &policy).await?;
    s.bus.publish(BusEvent::StorageChanged);
    s.retention.kick();
    Ok(Json(policy))
}

#[derive(serde::Deserialize)]
struct PathBody {
    path: String,
}

/// Switch the recordings folder (applies to new recordings immediately).
async fn update_path(State(s): State<AppState>, Json(body): Json<PathBody>) -> ApiResult<Json<serde_json::Value>> {
    let p = location::apply(&s.db, &s.recording_files, &body.path).await.map_err(crate::error::ApiError::invalid)?;
    s.bus.publish(BusEvent::StorageChanged);
    Ok(Json(serde_json::json!({ "path": p })))
}
