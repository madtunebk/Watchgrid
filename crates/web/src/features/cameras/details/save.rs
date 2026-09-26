//! Save flow for the camera settings tabs: build the full input from the
//! current camera, let the tab patch its part, send it.

use crate::api::{Camera, CameraInput};
use crate::features::cameras::mutations;
use crate::ui::SaveState;

/// Apply `patch` to the camera's current settings and save. Changes take
/// effect immediately on the NVR, without a restart.
pub fn camera(state: SaveState, camera: &Camera, patch: impl FnOnce(&mut CameraInput)) {
    let mut input = CameraInput::from(camera);
    patch(&mut input);
    state.run(mutations::update(camera.id.clone(), input));
}

pub use crate::ui::follow_server;
