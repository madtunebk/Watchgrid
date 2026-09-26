use leptos::prelude::*;
use leptos_router::components::A;

use super::actions_menu::ActionsMenu;
use crate::features::cameras::labels;
use super::preview::CameraPreview;
use super::record_button::RecordButton;
use crate::api::{Camera, CameraStatus};
use crate::features::events;
use crate::format;
use crate::ui::{Badge, I, Icon};

/// Overview card: identity, live preview, key settings, quick actions.
#[component]
pub fn CameraCard(camera: Camera) -> impl IntoView {
    let (tone, status) = labels::connection(&camera);
    let href = format!("/cameras/{}", camera.id);
    let id = camera.id.clone();
    let name = camera.name.clone();
    let open_label = format!("Open {name}");
    let (menu_name, camera_enabled) = (name.clone(), camera.enabled);
    let host = camera.host.clone();
    let motion_on = camera.motion.enabled;
    let mode = labels::mode_tag(camera.recording.mode);
    let recording = camera.recording_active;
    let camera_reason = camera.recording_reason;
    let can_record = camera.enabled && camera.status == CameraStatus::Online;
    let last_event = camera
        .last_event
        .as_ref()
        .map(|e| format!("{} · {}", events::title(e.kind), format::relative(e.time)))
        .unwrap_or_else(|| "None yet".into());

    view! {
        <article class="cam-card">
            <header class="cam-card__head">
                <A href=href.clone() attr:class="cam-card__name truncate">{name}</A>
                <Badge tone label=status dot=true />
            </header>

            <A href=href.clone() attr:class="cam-card__preview" attr:aria-label=open_label>
                {move || view! { <CameraPreview camera=camera.clone() substream=true /> }}
            </A>

            <dl class="cam-card__facts">
                <div class="fact fact--wide">
                    <dt>"Host"</dt>
                    <dd class="mono">{host}</dd>
                </div>
                <div class="fact">
                    <dt>"Motion detection"</dt>
                    <dd class:fact--on=motion_on>{if motion_on { "ON" } else { "OFF" }}</dd>
                </div>
                <div class="fact">
                    <dt>"Recording mode"</dt>
                    <dd>{mode}</dd>
                </div>
                <div class="fact fact--wide">
                    <dt>"Last event"</dt>
                    <dd class="truncate">{last_event}</dd>
                </div>
            </dl>

            <footer class="cam-card__actions">
                <A href=href attr:class="btn btn--sm btn--secondary">
                    <Icon icon=I::MonitorPlay class="icon icon--sm" />"Live"
                </A>
                <RecordButton camera_id=id.clone() recording reason=camera_reason available=can_record />
                <span class="cam-card__spacer"></span>
                <ActionsMenu camera_id=id camera_name=menu_name enabled=camera_enabled />
            </footer>
        </article>
    }
}
