//! Manual record toggle. The button stays busy until the refreshed camera
//! arrives and its owner re-renders.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::RecordingReason;
use crate::api::session::can_change;
use crate::features::cameras::mutations;
use crate::ui::{I, Icon};

#[component]
pub fn RecordButton(
    camera_id: String,
    recording: bool,
    /// Why the running clip records (automatic clips aren't stopped here).
    #[prop(default = None)]
    reason: Option<RecordingReason>,
    /// False when the camera cannot record (offline / disabled).
    available: bool,
    #[prop(optional)] compact: bool,
) -> impl IntoView {
    // Continuous / scheduled clips follow the camera's recording mode:
    // stopping one would only make the next start, so show that instead.
    if recording && matches!(reason, Some(RecordingReason::Continuous | RecordingReason::Scheduled)) {
        let what = if reason == Some(RecordingReason::Scheduled) { "scheduled" } else { "continuous" };
        return view! {
            <span class="btn btn--sm btn--record rec-btn rec-btn--auto" role="status"
                title=format!("Recording automatically ({what}). Change the camera's recording mode to stop it.")>
                <Icon icon=I::RecordDot class="icon icon--sm rec-btn__icon" />
                {(!compact).then_some("Auto")}
            </span>
        }
        .into_any();
    }
    // Viewers watch only.
    if !can_change() {
        return ().into_any();
    }
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
            // Stop stays possible when the camera dropped mid-recording.
            disabled=move || busy.get() || (!recording && !available)
            aria-pressed=recording.to_string()
            title=title
            on:click=toggle
        >
            <Icon icon=if recording { I::StopSquare } else { I::RecordDot } class="icon icon--sm rec-btn__icon" />
            {(!compact).then_some(label)}
        </button>
    }
    .into_any()
}
