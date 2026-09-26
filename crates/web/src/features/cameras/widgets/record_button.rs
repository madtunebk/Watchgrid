//! Manual record toggle. The button stays busy until the refreshed camera
//! arrives and its owner re-renders.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::features::cameras::mutations;
use crate::ui::{I, Icon};

#[component]
pub fn RecordButton(
    camera_id: String,
    recording: bool,
    /// False when the camera cannot record (offline / disabled).
    available: bool,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let failed = RwSignal::new(None::<String>);

    let toggle = move |_| {
        let id = camera_id.clone();
        busy.set(true);
        failed.set(None);
        spawn_local(async move {
            match mutations::set_recording(id, !recording).await {
                // The owner re-renders with fresh data; no need to reset `busy`.
                Ok(_) => {}
                Err(e) => {
                    failed.set(Some(e.to_string()));
                    busy.set(false);
                }
            }
        });
    };

    let label = if recording { "Stop" } else { "Record" };
    let title = move || {
        failed.get().unwrap_or_else(|| {
            if !available { "Camera is not online".into() } else if recording { "Stop recording".into() } else { "Start manual recording".into() }
        })
    };

    view! {
        <button
            class="btn btn--sm rec-btn"
            class:btn--record=recording
            class:btn--secondary=!recording
            class:rec-btn--error=move || failed.get().is_some()
            disabled=move || busy.get() || !available
            aria-pressed=recording.to_string()
            title=title
            on:click=toggle
        >
            <Icon icon=if recording { I::StopSquare } else { I::RecordDot } class="icon icon--sm rec-btn__icon" />
            {(!compact).then_some(label)}
        </button>
    }
}
