//! Cameras over HTTP — same functions as the mock's `cameras` module.

use super::client;
use crate::api::{ApiResult, Camera, CameraInput};

pub async fn list() -> ApiResult<Vec<Camera>> {
    client::get("/cameras").await
}

pub async fn get(id: &str) -> ApiResult<Camera> {
    client::get(&format!("/cameras/{id}")).await
}

pub async fn create(input: CameraInput) -> ApiResult<Camera> {
    client::post("/cameras", Some(&input)).await
}

pub async fn update(id: &str, input: CameraInput) -> ApiResult<Camera> {
    client::put(&format!("/cameras/{id}"), &input).await
}

pub async fn delete(id: &str) -> ApiResult<()> {
    client::delete(&format!("/cameras/{id}")).await
}

pub async fn set_enabled(id: &str, enabled: bool) -> ApiResult<Camera> {
    let action = if enabled { "enable" } else { "disable" };
    client::post(&format!("/cameras/{id}/{action}"), None::<&()>).await
}

pub async fn set_manual_recording(id: &str, on: bool) -> ApiResult<Camera> {
    let action = if on { "start" } else { "stop" };
    client::post(&format!("/cameras/{id}/recording/{action}"), None::<&()>).await
}
