//! `/api/v1/notifications` endpoints.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use watchgrid_model::{NOTIFICATION_BULK_MAX, NotificationBulkAction, NotificationBulkRequest, NotificationBulkResult, NotificationFilter, NotificationLevel, NotificationPage, Role, TestNotificationResult};

use super::rules::Draft;
use super::{repo, webhook};
use crate::auth::CurrentUser;
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
    camera: Option<String>,
    #[serde(default)]
    problems: bool,
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Read / unread are the signed-in user's own.
async fn list(State(s): State<AppState>, Extension(me): Extension<CurrentUser>, Query(q): Query<ListQuery>) -> ApiResult<Json<NotificationPage>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let filter = NotificationFilter { unread_only: q.unread, camera_id: q.camera.filter(|c| !c.is_empty()), problems_only: q.problems };
    Ok(Json(repo::page(&s.db, me.user_id, &filter, limit, q.offset.unwrap_or(0).max(0)).await?))
}

/// Read / unread change only the caller's state; deleting removes a
/// notification for everyone, so only administrators may.
async fn bulk(State(s): State<AppState>, Extension(me): Extension<CurrentUser>, Json(req): Json<NotificationBulkRequest>) -> ApiResult<Json<NotificationBulkResult>> {
    if req.ids.is_empty() || req.ids.len() > NOTIFICATION_BULK_MAX {
        return Err(ApiError::invalid(format!("Select between 1 and {NOTIFICATION_BULK_MAX} notifications")));
    }
    if req.action == NotificationBulkAction::Delete && me.role != Role::Admin {
        return Err(ApiError::forbidden("Only administrators can delete notifications (they are gone for everyone)"));
    }
    let changed = repo::bulk(&s.db, me.user_id, &req.ids, req.action).await?;
    s.bus.publish(BusEvent::NotificationsChanged);
    Ok(Json(NotificationBulkResult { changed }))
}

#[derive(Deserialize)]
struct Read {
    /// `None` = all.
    ids: Option<Vec<String>>,
}

async fn mark_read(State(s): State<AppState>, Extension(me): Extension<CurrentUser>, Json(body): Json<Read>) -> ApiResult<StatusCode> {
    repo::mark_read(&s.db, me.user_id, body.ids.as_deref()).await?;
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
