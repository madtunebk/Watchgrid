//! Saving a settings section: fetch the settings as the server has them
//! now, patch only this section, send the document, refresh everything that
//! shows settings. Fetching first means a section never writes back stale
//! values of the others (changed meanwhile in another tab or session).

use crate::api::{self, Settings, Topic, invalidate};
use crate::ui::SaveState;

pub fn save(state: SaveState, _current: &Settings, patch: impl FnOnce(&mut Settings) + 'static) {
    state.run(async move {
        let mut next = api::get_settings().await?;
        patch(&mut next);
        let saved = api::update_settings(next).await?;
        invalidate(Topic::Settings);
        Ok(saved)
    });
}
