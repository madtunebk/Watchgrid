//! `/api/v1/notifications` endpoints.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use watchgrid_model::{NOTIFICATION_BULK_MAX, NotificationBulkRequest, NotificationBulkResult, NotificationPage};

use super::repo;
use crate::bus::BusEvent;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list)).route("/read", post(mark_read)).route("/bulk", post(bulk))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    #[serde(default)]
    unread: bool,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<NotificationPage>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(repo::page(&s.db, q.unread, limit, q.offset.unwrap_or(0).max(0)).await?))
}

async fn bulk(State(s): State<AppState>, Json(req): Json<NotificationBulkRequest>) -> ApiResult<Json<NotificationBulkResult>> {
    if req.ids.is_empty() || req.ids.len() > NOTIFICATION_BULK_MAX {
        return Err(ApiError::invalid(format!("Select between 1 and {NOTIFICATION_BULK_MAX} notifications")));
    }
    let changed = repo::bulk(&s.db, &req.ids, req.action).await?;
    s.bus.publish(BusEvent::NotificationsChanged);
    Ok(Json(NotificationBulkResult { changed }))
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
