use super::{ApiResult, Id, PtzMove, PtzPreset, PtzState, backend};

/// GET /api/v1/cameras/{id}/ptz — whether the camera moves, and its presets.
pub async fn get_ptz(camera_id: Id) -> ApiResult<PtzState> {
    backend::ptz::state(&camera_id).await
}

/// POST /api/v1/cameras/{id}/ptz/move — keeps going ~1 s; repeat while held.
pub async fn ptz_move(camera_id: Id, m: PtzMove) -> ApiResult<()> {
    backend::ptz::move_at(&camera_id, m).await
}

/// POST /api/v1/cameras/{id}/ptz/stop
pub async fn ptz_stop(camera_id: Id) -> ApiResult<()> {
    backend::ptz::stop(&camera_id).await
}

/// POST /api/v1/cameras/{id}/ptz/presets/{token}/goto
pub async fn ptz_goto(camera_id: Id, token: String) -> ApiResult<()> {
    backend::ptz::goto(&camera_id, &token).await
}

/// POST /api/v1/cameras/{id}/ptz/presets — save the current position.
pub async fn ptz_save_preset(camera_id: Id, name: String) -> ApiResult<PtzPreset> {
    backend::ptz::save(&camera_id, &name).await
}

/// DELETE /api/v1/cameras/{id}/ptz/presets/{token}
pub async fn ptz_remove_preset(camera_id: Id, token: String) -> ApiResult<()> {
    backend::ptz::remove(&camera_id, &token).await
}
