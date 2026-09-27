use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{Camera, CameraStatus, StreamStatus};
use crate::features::cameras::{labels, render_key};
use crate::features::cameras::widgets::{ActionsMenu, RecordButton};
use crate::features::events;
use crate::format;
use crate::ui::{Badge, Dot, I, Icon, Tone};

/// Dense list view: one row per camera with every state column.
#[component]
pub fn CameraTable(#[prop(into)] cameras: Signal<Vec<Camera>>) -> impl IntoView {
    view! {
        <div class="table-wrap">
            <table class="table">
                <thead>
                    <tr>
                        <th>"Camera"</th>
                        <th>"Status"</th>
                        <th class="col-host">"Host"</th>
                        <th class="col-rtsp">"RTSP"</th>
                        <th>"Recording"</th>
                        <th class="col-mode">"Mode"</th>
                        <th>"Motion"</th>
                        <th class="col-last">"Last event"</th>
                        <th class="num col-storage">"Storage"</th>
                        <th class="actions"><span class="sr-only">"Actions"</span></th>
                    </tr>
                </thead>
                <tbody>
                    <For each=move || cameras.get() key=render_key let:camera><Row camera /></For>
                </tbody>
            </table>
        </div>
    }
}

fn stream_tone(s: StreamStatus) -> Tone {
    match s {
        StreamStatus::Active => Tone::Stream,
        StreamStatus::Idle | StreamStatus::Unconfigured => Tone::Offline,
        StreamStatus::Error => Tone::Danger,
    }
}

/// What a stream's dot means; an idle substream is normal (opened on demand).
fn stream_title(name: &str, s: StreamStatus) -> String {
    let state = match s {
        StreamStatus::Active => "in use",
        StreamStatus::Idle => "idle, opened only when something needs it",
        StreamStatus::Unconfigured => "not set up",
        StreamStatus::Error => "error",
    };
    format!("{name}: {state}")
}

#[component]
fn Row(camera: Camera) -> impl IntoView {
    let (tone, status) = labels::connection(&camera);
    let href = format!("/cameras/{}", camera.id);
    let can_record = camera.enabled && camera.status == CameraStatus::Online;
    let main_tone = stream_tone(camera.main_stream.status);
    let main_title = stream_title("Main stream", camera.main_stream.status);
    let sub = camera.sub_stream.as_ref().map(|s| (stream_tone(s.status), stream_title("Substream", s.status)));
    let (id, name, menu_name) = (camera.id.clone(), camera.name.clone(), camera.name.clone());
    let edit_href = format!("/cameras/{}/edit", camera.id);
    let last = camera.last_event.as_ref().map(|e| (events::title(e.kind), format::relative(e.time)));

    view! {
        <tr class:row--disabled=!camera.enabled>
            <td>
                <A href=href.clone() attr:class="table__primary">{name}</A>
                <div class="table__secondary">{camera.location.clone()}</div>
            </td>
            <td><Badge tone label=status dot=true /></td>
            <td class="mono col-host">{camera.host.clone()}</td>
            <td class="col-rtsp">
                <span class="rtsp">
                    <span class="rtsp__item" title=main_title><Dot tone=main_tone />"Main"</span>
                    {match sub {
                        Some((t, title)) => view! { <span class="rtsp__item" title=title><Dot tone=t />"Sub"</span> }.into_any(),
                        None => view! { <span class="rtsp__item rtsp__item--none" title="No substream configured">"—"</span> }.into_any(),
                    }}
                </span>
            </td>
            <td>
                {if camera.recording_active {
                    view! { <Badge tone=Tone::Recording label="REC" dot=true pulse=true /> }.into_any()
                } else {
                    view! { <span class="muted">"Idle"</span> }.into_any()
                }}
            </td>
            <td class="col-mode">{labels::mode_tag(camera.recording.mode)}</td>
            <td>
                {match (camera.motion.enabled, camera.motion_active) {
                    (true, true) => view! { <Badge tone=Tone::Motion label="MOTION" dot=true /> }.into_any(),
                    (true, false) => view! { <span class="text-online">"ON"</span> }.into_any(),
                    (false, _) => view! { <span class="muted">"OFF"</span> }.into_any(),
                }}
            </td>
            <td class="col-last">
                {match last {
                    Some((title, when)) => view! { <div>{title}</div><div class="table__secondary">{when}</div> }.into_any(),
                    None => view! { <span class="muted">"—"</span> }.into_any(),
                }}
            </td>
            <td class="num mono col-storage">{camera.storage_used.map(format::bytes).unwrap_or_else(|| "—".into())}</td>
            <td class="actions">
                <div class="row-actions">
                    <A href=href attr:class="icon-btn icon-btn--sm" attr:title="Live view" attr:aria-label="Live view">
                        <Icon icon=I::MonitorPlay />
                    </A>
                    <RecordButton camera_id=id.clone() recording=camera.recording_active reason=camera.recording_reason available=can_record compact=true />
                    <A href=edit_href attr:class="icon-btn icon-btn--sm row-actions__secondary" attr:title="Edit" attr:aria-label="Edit">
                        <Icon icon=I::Pencil />
                    </A>
                    <ActionsMenu camera_id=id camera_name=menu_name enabled=camera.enabled />
                </div>
            </td>
        </tr>
    }
}
