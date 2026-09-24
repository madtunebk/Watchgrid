use super::{ApiResult, Settings, backend};

/// GET /api/v1/settings
pub async fn get_settings() -> ApiResult<Settings> {
    backend::settings::get().await
}

/// PUT /api/v1/settings — applied live.
pub async fn update_settings(settings: Settings) -> ApiResult<Settings> {
    backend::settings::update(settings).await
}
