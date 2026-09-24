//! Notifications over HTTP — same functions as the mock's `notifications` module.

use super::client;
use crate::api::{ApiResult, Id, Notification};

pub async fn list() -> ApiResult<Vec<Notification>> {
    client::get("/notifications").await
}

pub async fn mark_read(ids: Option<Vec<Id>>) -> ApiResult<()> {
    client::post_json_no_content("/notifications/read", &serde_json::json!({ "ids": ids })).await
}
