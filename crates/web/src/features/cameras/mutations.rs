//! Camera write operations as used by the UI: call the API, then refresh
//! every query the change affects. Components never invalidate by hand.

use crate::api::{self, ApiResult, Camera, CameraInput, Topic, invalidate};

fn refresh_after_camera_change() {
    invalidate(Topic::Cameras);
    invalidate(Topic::System);
    invalidate(Topic::Storage);
}

pub async fn create(input: CameraInput) -> ApiResult<Camera> {
    let cam = api::create_camera(input).await?;
    refresh_after_camera_change();
    Ok(cam)
}

pub async fn update(id: String, input: CameraInput) -> ApiResult<Camera> {
    let cam = api::update_camera(id, input).await?;
    refresh_after_camera_change();
    Ok(cam)
}

pub async fn delete(id: String) -> ApiResult<()> {
    api::delete_camera(id).await?;
    refresh_after_camera_change();
    invalidate(Topic::Events);
    Ok(())
}

pub async fn set_enabled(id: String, enabled: bool) -> ApiResult<Camera> {
    let cam = api::set_camera_enabled(id, enabled).await?;
    refresh_after_camera_change();
    Ok(cam)
}

pub async fn set_recording(id: String, on: bool) -> ApiResult<Camera> {
    let cam = if on { api::start_recording(id).await? } else { api::stop_recording(id).await? };
    invalidate(Topic::Cameras);
    invalidate(Topic::System);
    Ok(cam)
}
