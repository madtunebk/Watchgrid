use leptos::prelude::*;

use crate::api::Camera;
use crate::features::cameras::labels;
use crate::ui::{Badge, Tone};

/// The four independent camera states, each with its own colour:
/// connection, live stream, motion, recording.
#[component]
pub fn StateBadges(camera: Camera) -> impl IntoView {
    let (tone, label) = labels::connection(&camera);
    view! {
        <div class="state-badges">
            <Badge tone label dot=true />
            {camera.streaming().then(|| view! { <Badge tone=Tone::Stream label="LIVE" /> })}
            {camera.motion_active.then(|| view! { <Badge tone=Tone::Motion label="MOTION" dot=true /> })}
            {camera.recording_active.then(|| view! { <Badge tone=Tone::Recording label="REC" dot=true pulse=true /> })}
        </div>
    }
}
