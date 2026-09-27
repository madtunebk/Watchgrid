//! Notifications over HTTP — same functions as the mock's `notifications` module.

use super::client;
use crate::api::{ApiResult, Id, NotificationBulkRequest, NotificationFilter, NotificationBulkResult, NotificationPage, TestNotificationResult};

pub async fn list(f: &NotificationFilter, limit: u32, offset: u32) -> ApiResult<NotificationPage> {
    let camera = f.camera_id.as_deref().map(|c| format!("&camera={}", String::from(js_sys::encode_uri_component(c)))).unwrap_or_default();
    client::get(&format!("/notifications?unread={}&problems={}{camera}&limit={limit}&offset={offset}", f.unread_only, f.problems_only)).await
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
