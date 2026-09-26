//! `/api/v1/notifications` endpoints.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use watchgrid_model::{NOTIFICATION_BULK_MAX, NotificationBulkRequest, NotificationBulkResult, NotificationLevel, NotificationPage, TestNotificationResult};

use super::rules::Draft;
use super::{repo, webhook};
use crate::bus::BusEvent;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list)).route("/read", post(mark_read)).route("/bulk", post(bulk)).route("/test", post(test))
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

/// A notification on demand, the same way real ones go: stored, shown in
/// the bell, and sent to the saved webhook (whose answer is returned).
async fn test(State(s): State<AppState>) -> ApiResult<Json<TestNotificationResult>> {
    let settings = crate::settings::load_app(&s.db, s.bind).await?;
    let draft = Draft {
        kind: "test",
        camera_id: None,
        level: NotificationLevel::Info,
        title: "Test notification".into(),
        message: "Sent from Settings → Notifications. If you see this, notifications work.".into(),
        link: Some("/notifications".into()),
    };
    let stored = repo::insert(&s.db, &draft).await?;
    s.bus.publish(BusEvent::NotificationsChanged);
    let Some(url) = settings.notifications.webhook_url else { return Ok(Json(TestNotificationResult::default())) };
    Ok(Json(match webhook::send(&url, &stored, &settings.general.nvr_name).await {
        Ok(status) => TestNotificationResult { webhook: Some(format!("HTTP {status}")), webhook_ok: true },
        Err(e) => TestNotificationResult { webhook: Some(e), webhook_ok: false },
    }))
}
