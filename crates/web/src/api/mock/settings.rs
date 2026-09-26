//! Mock settings. Renaming the NVR also renames the server, like the real one.

use super::db::with_db;
use super::sim::latency;
use crate::api::query::{Topic, invalidate};
use crate::api::{ApiError, ApiResult, Settings};

pub async fn get() -> ApiResult<Settings> {
    latency().await;
    Ok(with_db(|db| db.settings.clone()))
}

pub async fn update(settings: Settings) -> ApiResult<Settings> {
    latency().await;
    if settings.general.nvr_name.trim().is_empty() {
        return Err(ApiError::new(422, "invalid", "The NVR needs a name"));
    }
    if settings.network.http_port == 0 {
        return Err(ApiError::new(422, "invalid", "HTTP port must be between 1 and 65535"));
    }
    let saved = with_db(|db| {
        db.server.name = settings.general.nvr_name.trim().to_string();
        db.settings = settings;
        db.settings.clone()
    });
    invalidate(Topic::Server);
    Ok(saved)
}
