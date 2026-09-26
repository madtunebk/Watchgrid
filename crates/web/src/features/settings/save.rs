//! Saving a settings section: patch the current settings, send the whole
//! document, refresh everything that shows settings.

use crate::api::{self, Settings, Topic, invalidate};
use crate::ui::SaveState;

pub fn save(state: SaveState, current: &Settings, patch: impl FnOnce(&mut Settings)) {
    let mut next = current.clone();
    patch(&mut next);
    state.run(async move {
        let saved = api::update_settings(next).await?;
        invalidate(Topic::Settings);
        Ok(saved)
    });
}
