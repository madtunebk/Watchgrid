//! Side panel that plays a recording and links to its events.

use leptos::ev;
use leptos::prelude::*;
use leptos_router::components::A;

use super::labels;
use crate::api::{self, Recording};
use crate::features::playback::{Clip, Player};
use crate::format;
use crate::ui::{I, Icon};

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
        ("Protected", if recording.protected { "Yes" } else { "No" }.to_string()),
    ];
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
