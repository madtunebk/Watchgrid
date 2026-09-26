//! Save flow for the camera settings tabs: build the full input from the
//! current camera, let the tab patch its part, send it.

use leptos::prelude::*;

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

/// Keep an untouched form in step with the server: when the saved values
/// change (another tab, another session) and the form still shows the old
/// ones, `revert` loads the new ones. A form with local edits is left alone.
pub fn follow_server<T: PartialEq + Clone + Send + Sync + 'static>(
    draft: impl Fn() -> T + Send + Sync + 'static,
    saved: impl Fn() -> T + Send + Sync + 'static,
    revert: Callback<()>,
) {
    let last = StoredValue::new(untrack(&saved));
    Effect::new(move |_| {
        let now = saved();
        let before = last.get_value();
        if now != before {
            if untrack(&draft) == before {
                revert.run(());
            }
            last.set_value(now);
        }
    });
}
