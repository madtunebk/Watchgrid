//! Video area with live-state overlays. With the real server (`live-api`)
//! a streaming camera shows live video; the mock build shows a placeholder.

use std::time::Duration;

use leptos::prelude::*;

use crate::features::cameras::labels;
use crate::api::{Camera, CameraStatus};
use crate::clock::use_now;
use crate::ui::{I, Icon};

#[component]
pub fn CameraPreview(
    camera: Camera,
    /// Play the substream (small tiles). Ignored when the camera has none.
    #[prop(optional)]
    substream: bool,
) -> impl IntoView {
    let streaming = camera.streaming();
    let connecting = camera.enabled && camera.status == CameraStatus::Connecting;
    let offline = camera.enabled && !connecting && camera.status != CameraStatus::Online;
    let sub = camera.sub_stream.as_ref().filter(|_| substream);
    let summary = labels::stream_summary(sub.unwrap_or(&camera.main_stream)).map(|s| if sub.is_some() { format!("{s} · SUB") } else { s });
    let now = use_now(Duration::from_secs(1));

    let surface = if !camera.enabled {
        view! { <div class="preview__state"><Icon icon=I::VideoOff class="icon icon--xl" /><span>"Camera disabled"</span></div> }.into_any()
    } else if connecting {
        view! { <div class="preview__state"><Icon icon=I::Loader class="icon icon--xl spin" /><span>"Connecting…"</span><small>"Opening the RTSP stream"</small></div> }.into_any()
    } else if offline {
        view! { <div class="preview__state preview__state--offline"><Icon icon=I::VideoOff class="icon icon--xl" /><span>"Camera offline"</span><small>"Reconnecting…"</small></div> }.into_any()
    } else {
        live_surface(&camera, sub.is_some())
    };

    view! {
        <div class="preview" class:preview--live=streaming>
            {surface}
            <div class="preview__tags">
                {streaming.then(|| view! { <span class="ptag ptag--live">"LIVE"</span> })}
                {camera.motion_active.then(|| view! { <span class="ptag ptag--motion">"MOTION"</span> })}
                {camera.recording_active.then(|| view! { <span class="ptag ptag--rec"><span class="ptag__dot pulse"></span>"REC"</span> })}
            </div>
            {streaming.then(|| view! {
                <div class="preview__osd">
                    <span>{camera.name.clone()}</span>
                    <span>{move || crate::format::date_time(now.get())}</span>
                </div>
                {summary.map(|s| view! { <span class="preview__res">{s}</span> })}
            })}
        </div>
    }
}

#[cfg(feature = "live-api")]
fn live_surface(camera: &Camera, substream: bool) -> AnyView {
    use crate::live_video::LiveVideo;
    view! { <LiveVideo camera_id=camera.id.clone() substream /> }.into_any()
}

#[cfg(not(feature = "live-api"))]
fn live_surface(_camera: &Camera, _substream: bool) -> AnyView {
    view! { <div class="preview__state preview__state--live"><Icon icon=I::Cctv class="icon icon--xl" /></div> }.into_any()
}
