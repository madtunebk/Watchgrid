//! Settings over HTTP — same functions as the mock's `settings` module.

use super::client;
use crate::api::query::{Topic, invalidate};
use crate::api::{ApiResult, Settings};

pub async fn get() -> ApiResult<Settings> {
    client::get("/settings").await
}

/// The NVR name is also the server's name, so refresh that too.
pub async fn update(settings: Settings) -> ApiResult<Settings> {
    let saved = client::put("/settings", &settings).await?;
    invalidate(Topic::Server);
    Ok(saved)
}
