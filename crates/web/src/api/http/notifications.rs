//! Notifications over HTTP — same functions as the mock's `notifications` module.

use super::client;
use crate::api::{ApiResult, Id, NotificationBulkRequest, NotificationBulkResult, NotificationPage, TestNotificationResult};

pub async fn list(unread_only: bool, limit: u32, offset: u32) -> ApiResult<NotificationPage> {
    client::get(&format!("/notifications?unread={unread_only}&limit={limit}&offset={offset}")).await
}

pub async fn mark_read(ids: Option<Vec<Id>>) -> ApiResult<()> {
    client::post_json_no_content("/notifications/read", &serde_json::json!({ "ids": ids })).await
}

pub async fn bulk(req: &NotificationBulkRequest) -> ApiResult<NotificationBulkResult> {
    client::post("/notifications/bulk", Some(req)).await
}

pub async fn test() -> ApiResult<TestNotificationResult> {
    client::post::<TestNotificationResult>("/notifications/test", None::<&()>).await
}
