//! One camera over the wall (CAM_FS). The grid stays mounted underneath,
//! so closing this returns to the same grid, layout and live streams.

use std::collections::HashMap;

use leptos::prelude::*;

use super::tile::Tile;
use super::view_state::WallView;
use crate::api::Camera;
use crate::ui::{I, Icon};

#[component]
pub fn CameraOverlay(view: WallView, #[prop(into)] cameras: Signal<HashMap<String, Camera>>) -> impl IntoView {
    let close = Callback::new(move |_| view.close());
    move || {
        view.focused.get().map(|id| {
            let camera = Signal::derive(move || cameras.get().get(&id).cloned());
            view! {
                <div class="live-focus" role="dialog" aria-label="Camera">
                    <div class="live-focus__stage">
                        <Tile camera on_focus=close on_remove=close on_fullscreen=Callback::new(move |_| view.toggle_wall_fullscreen()) />
                    </div>
                    <button class="btn btn--secondary btn--sm live-focus__back" title="Back (Esc, browser Back, Alt+←)" on:click=move |_| view.close()>
                        <Icon icon=I::ArrowLeft class="icon icon--sm" />
                        {move || if view.fullscreen.get() { "Back to fullscreen grid" } else { "Back to grid" }}
                    </button>
                </div>
            }
        })
    }
}
