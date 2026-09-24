use super::{ApiResult, RetentionPolicy, StorageStatus, backend};

/// GET /api/v1/storage
pub async fn get_storage_status() -> ApiResult<StorageStatus> {
    backend::storage::status().await
}

/// PUT /api/v1/storage/retention — protected recordings are never deleted.
pub async fn update_retention(policy: RetentionPolicy) -> ApiResult<()> {
    backend::storage::update_retention(policy).await
}
