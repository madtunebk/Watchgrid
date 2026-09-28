use super::{ApiResult, ArmInput, ArmState, Id, backend};
use crate::api::query::{Topic, invalidate};

/// GET /api/v1/arm — whether cameras are armed, since when, which.
pub async fn get_arm() -> ApiResult<ArmState> {
    backend::arm::state().await
}

/// POST /api/v1/arm — arm `cameras` (empty = all), adding to those armed.
pub async fn arm_cameras(cameras: Vec<Id>) -> ApiResult<ArmState> {
    let state = backend::arm::change(ArmInput { armed: true, cameras }).await;
    invalidate(Topic::Cameras);
    state
}

/// POST /api/v1/arm — disarm every armed camera (each back as it was).
pub async fn disarm_cameras() -> ApiResult<ArmState> {
    let state = backend::arm::change(ArmInput { armed: false, cameras: Vec::new() }).await;
    invalidate(Topic::Cameras);
    state
}
