//! One live video tile with hover controls.

use std::time::Duration;

use leptos::html::Div;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{Camera, CameraStatus};
use crate::features::cameras::{CameraPreview, RecordButton};
use crate::ui::{I, Icon, snapshot as snap};

#[component]
pub fn Tile(
    #[prop(into)] camera: Signal<Option<Camera>>,
    /// Play the substream when the camera has one (small tiles).
    #[prop(optional)]
    substream: bool,
    /// Expand this tile to single-camera view.
    on_focus: Callback<()>,
    /// Remove the camera from this tile.
    on_remove: Callback<()>,
    /// Fullscreen button, shown only on the opened camera (in the grid, the
    /// single-view button covers it). Tiles never request browser fullscreen
    /// themselves: the wall owns it (see `view_state`).
    #[prop(optional)]
    on_fullscreen: Option<Callback<()>>,
) -> impl IntoView {
    let el = NodeRef::<Div>::new();
    let muted = RwSignal::new(true);
    let flash = RwSignal::new(false);
    let toast = RwSignal::new(String::new());

    let snapshot = move |_| {
        let (Some(el), Some(c)) = (el.get_untracked(), camera.get_untracked()) else { return };
        match snap::save_frame(&el, &snap::file_name(&c.name)) {
            Ok(()) => toast.set("Snapshot saved".into()),
            Err(e) => toast.set(e.into()),
        }
        flash.set(true);
        set_timeout(move || flash.set(false), Duration::from_millis(1600));
    };


    view! {
        <div class="tile" node_ref=el class:tile--flash=flash on:dblclick=move |_| on_focus.run(())>
            {move || camera.get().map(|c| view! { <CameraPreview camera=c substream /> })}
            <Show when=move || flash.get()>
                <span class="tile__toast">{move || toast.get()}</span>
            </Show>
            <div class="tile__controls" on:dblclick=|ev| ev.stop_propagation()>
                {on_fullscreen.map(|f| view! {
                    <button class="tile__btn" title="Fullscreen" aria-label="Fullscreen" on:click=move |_| f.run(())>
                        <Icon icon=I::Maximize class="icon icon--sm" />
                    </button>
                })}
                {move || camera.get().map(|c| {
                    let available = c.enabled && c.status == CameraStatus::Online;
                    view! { <RecordButton camera_id=c.id recording=c.recording_active available compact=true /> }
                })}
                <button class="tile__btn" title="Snapshot" aria-label="Snapshot" on:click=snapshot>
                    <Icon icon=I::Camera class="icon icon--sm" />
                </button>
                <button class="tile__btn" aria-label=move || if muted.get() { "Unmute" } else { "Mute" }
                    title=move || if muted.get() { "Unmute" } else { "Mute" }
                    on:click=move |_| muted.update(|m| *m = !*m)>
                    {move || view! { <Icon icon=if muted.get() { I::VolumeOff } else { I::Volume } class="icon icon--sm" /> }}
                </button>
                <span class="tile__spacer"></span>
                {on_fullscreen.is_none().then(|| view! {
                    <button class="tile__btn" title="Single view (double-click)" aria-label="Single view" on:click=move |_| on_focus.run(())>
                        <Icon icon=I::Expand class="icon icon--sm" />
                    </button>
                })}
                {move || camera.get().map(|c| view! {
                    <A href=format!("/cameras/{}", c.id) attr:class="tile__btn" attr:title="Camera details" attr:aria-label="Camera details">
                        <Icon icon=I::Eye class="icon icon--sm" />
                    </A>
                })}
                {on_fullscreen.is_none().then(|| view! {
                    <button class="tile__btn" title="Remove from grid" aria-label="Remove from grid" on:click=move |_| on_remove.run(())>
                        <Icon icon=I::X class="icon icon--sm" />
                    </button>
                })}
            </div>
        </div>
    }
}
