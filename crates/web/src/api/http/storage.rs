//! Storage over HTTP — same functions as the mock's `storage` module.

use super::client;
use crate::api::{ApiResult, RetentionPolicy, StorageStatus};

pub async fn status() -> ApiResult<StorageStatus> {
    client::get("/storage").await
}

pub async fn update_retention(policy: RetentionPolicy) -> ApiResult<()> {
    client::put::<RetentionPolicy>("/storage/retention", &policy).await.map(|_| ())
}

pub async fn set_path(path: &str) -> ApiResult<()> {
    client::put::<serde_json::Value>("/storage/path", &serde_json::json!({ "path": path })).await.map(|_| ())
}
