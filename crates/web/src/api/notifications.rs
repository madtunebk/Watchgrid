use super::{ApiResult, Id, Notification, backend};

/// GET /api/v1/notifications
pub async fn get_notifications() -> ApiResult<Vec<Notification>> {
    backend::notifications::list().await
}

/// POST /api/v1/notifications/read   (`None` = all)
pub async fn mark_notifications_read(ids: Option<Vec<Id>>) -> ApiResult<()> {
    backend::notifications::mark_read(ids).await
}
