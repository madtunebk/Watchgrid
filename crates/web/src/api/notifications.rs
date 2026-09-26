use super::{ApiResult, Id, NotificationBulkRequest, NotificationBulkResult, NotificationPage, TestNotificationResult, backend};

/// GET /api/v1/notifications?unread=&limit=&offset= — newest first, with
/// the overall unread count.
pub async fn get_notifications(unread_only: bool, limit: u32, offset: u32) -> ApiResult<NotificationPage> {
    backend::notifications::list(unread_only, limit, offset).await
}

/// POST /api/v1/notifications/read   (`None` = all)
pub async fn mark_notifications_read(ids: Option<Vec<Id>>) -> ApiResult<()> {
    backend::notifications::mark_read(ids).await
}

/// POST /api/v1/notifications/bulk — mark read / unread or delete many.
pub async fn notifications_bulk(req: NotificationBulkRequest) -> ApiResult<NotificationBulkResult> {
    backend::notifications::bulk(&req).await
}

/// POST /api/v1/notifications/test — one notification on demand, also sent
/// to the saved webhook.
pub async fn send_test_notification() -> ApiResult<TestNotificationResult> {
    backend::notifications::test().await
}
