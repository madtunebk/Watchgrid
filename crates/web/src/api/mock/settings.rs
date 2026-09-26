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
    let saved = with_db(|db| {
        db.server.name = settings.general.nvr_name.trim().to_string();
        // Read-only, like on the server.
        let network = db.settings.network.clone();
        db.settings = Settings { network, ..settings };
        db.settings.clone()
    });
    invalidate(Topic::Server);
    Ok(saved)
}
