//! `/api/v1/notifications` endpoints.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use watchgrid_model::Notification;

use super::repo;
use crate::bus::BusEvent;
use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list)).route("/read", post(mark_read))
}

async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<Notification>>> {
    Ok(Json(repo::list(&s.db, 100).await?))
}

#[derive(Deserialize)]
struct Read {
    /// `None` = all.
    ids: Option<Vec<String>>,
}

async fn mark_read(State(s): State<AppState>, Json(body): Json<Read>) -> ApiResult<StatusCode> {
    repo::mark_read(&s.db, body.ids.as_deref()).await?;
    s.bus.publish(BusEvent::NotificationsChanged);
    Ok(StatusCode::NO_CONTENT)
}
