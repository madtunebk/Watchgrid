//! Side panel that plays a recording, links to its events and exports or
//! protects it.

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use super::labels;
use crate::api::{self, Recording};
use crate::features::clip_export::{ExportMenu, ExportSubject};
use crate::features::playback::{Clip, Player};
use crate::format;
use crate::ui::{ConfirmDialog, I, Icon, Tone, use_toaster};

#[component]
pub fn RecordingDrawer(recording: Recording, camera_name: String, on_close: Callback<()>) -> impl IntoView {
    let esc = window_event_listener(ev::keydown, move |e| if e.key() == "Escape" { on_close.run(()) });
    on_cleanup(move || esc.remove());

    // The first event (if any) sets the highlighted part of the clip.
    let first = recording.event_ids.first().cloned();
    let detail = LocalResource::new(move || {
        let id = first.clone();
        async move {
            match id {
                Some(id) => api::get_event(id).await.ok().map(|d| d.event),
                None => None,
            }
        }
    });

    let t = |x: chrono::DateTime<chrono::Utc>| {
        let x = x.with_timezone(&chrono::Local);
        format!("{}, {}", x.format("%a %-d %b"), crate::format::time_hms(x))
    };
    let rows = vec![
        ("Camera", camera_name.clone()),
        ("Reason", labels::label(recording.reason).to_string()),
        ("Started", t(recording.start_time)),
        ("Ended", recording.end_time.map(t).unwrap_or_else(|| "Recording now".into())),
        ("Length", format::duration(recording.duration)),
        ("Size", format::bytes(recording.file_size)),
    ];
    // Finished clips only: one still recording has no file yet.
    let saved = recording.end_time.is_some();
    let subject = ExportSubject::Recording { id: recording.id.clone(), camera_id: recording.camera_id.clone(), start: recording.start_time };
    // Manual protection; protected events keep the clip regardless.
    let protected = RwSignal::new(recording.protected);
    let by_events = recording.protected_by_events;
    let inherited = by_events > 0;
    let busy = RwSignal::new(false);
    let protect_error = RwSignal::new(None::<String>);
    let confirm_delete = RwSignal::new(false);
    let toaster = use_toaster();
    let do_delete = Callback::new({
        let id = recording.id.clone();
        move |_| {
            let id = id.clone();
            busy.set(true);
            spawn_local(async move {
                match api::delete_recording(id).await {
                    Ok(()) => {
                        for topic in [api::Topic::Recordings, api::Topic::Events, api::Topic::Storage] {
                            api::invalidate(topic);
                        }
                        confirm_delete.set(false);
                        toaster.show(Tone::Online, "Recording deleted");
                        on_close.run(());
                    }
                    Err(e) => {
                        confirm_delete.set(false);
                        toaster.show(Tone::Danger, format!("Couldn't delete the recording: {e}"));
                    }
                }
                busy.set(false);
            });
        }
    });
    let events_count = recording.event_ids.len();
    let delete_message = match events_count {
        0 => "The video file will be deleted permanently.".to_string(),
        1 => "The video file and its event will be deleted permanently.".to_string(),
        n => format!("The video file and its {n} events will be deleted permanently."),
    };
    let toggle_protect = {
        let id = recording.id.clone();
        move |_| {
            let (id, next) = (id.clone(), !protected.get_untracked());
            busy.set(true);
            spawn_local(async move {
                match api::set_recording_protected(id, next).await {
                    Ok(()) => protected.set(next),
                    Err(e) => protect_error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };
    let rec = recording.clone();
    let cam_id = recording.camera_id.clone();
    let single = recording.event_ids.len() == 1;
    let event_links: Vec<(usize, String)> = recording.event_ids.iter().cloned().enumerate().collect();
    let (title, sub) = (format!("{} recording", labels::label(recording.reason)), camera_name.clone());

    view! {
        <div class="drawer-backdrop" on:click=move |_| on_close.run(())></div>
        <aside class="drawer" role="dialog" aria-label="Recording">
            <header class="drawer__head">
                <div>
                    <h2 class="drawer__title">{title}</h2>
                    <p class="drawer__sub">{sub}</p>
                </div>
                <button class="icon-btn" aria-label="Close" on:click=move |_| on_close.run(())><Icon icon=I::X /></button>
            </header>
            <div class="drawer__body">
                {move || detail.get().map(|event| {
                    let clip = Clip::for_recording(&rec, event.as_ref(), camera_name.clone());
                    view! { <Player clip /> }
                })}
                <dl class="facts">
                    {rows.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd>{v}</dd></div> }).collect_view()}
                </dl>
                {saved.then(|| view! {
                    <div class="drawer__actions">
                        <ExportMenu subject=subject.clone() />
                        <button class="btn" class:btn--primary=protected class:btn--secondary=move || !protected.get() disabled=busy on:click=toggle_protect.clone()
                            title="Protected recordings are never deleted by retention">
                            {move || view! { <Icon icon=if protected.get() { I::Lock } else { I::LockOpen } class="icon icon--sm" /> }}
                            {move || if protected.get() { "Protected" } else { "Protect" }}
                        </button>
                        <button class="btn btn--danger" disabled=move || busy.get() || protected.get() || inherited
                            title=move || if protected.get() || inherited { "Remove the protection before deleting" } else { "" }
                            on:click=move |_| confirm_delete.set(true)>
                            <Icon icon=I::Trash class="icon icon--sm" />"Delete"
                        </button>
                        {inherited.then(|| view! {
                            <p class="drawer__note">
                                {if by_events == 1 { "Also kept by a protected event.".to_string() } else { format!("Also kept by {by_events} protected events.") }}
                            </p>
                        })}
                        {move || protect_error.get().map(|e| view! { <p class="drawer__note">{e}</p> })}
                    </div>
                    <ConfirmDialog open=confirm_delete title="Delete recording?" confirm_label="Delete recording" danger=true busy
                        message=delete_message.clone() on_confirm=do_delete />
                })}
                <div class="drawer__links">
                    {event_links.into_iter().map(|(i, id)| view! {
                        <A href=format!("/events/{id}") attr:class="btn btn--secondary btn--sm">
                            <Icon icon=I::Activity class="icon icon--sm" />
                            {if single { "Open event".to_string() } else { format!("Event {}", i + 1) }}
                        </A>
                    }).collect_view()}
                    <A href=format!("/cameras/{cam_id}") attr:class="btn btn--secondary btn--sm">
                        <Icon icon=I::Cctv class="icon icon--sm" />"Open camera"
                    </A>
                </div>
            </div>
        </aside>
    }
}
