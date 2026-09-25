//! Event details: `/events/:id` — player, facts, actions, older/newer.

mod actions;
mod export;
mod facts;

use leptos::ev;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::api::{self, ApiError, EventType, Topic, use_query};
use crate::features::playback::{Clip, Player};
use crate::features::events::{labels, list::BACK_KEY};
use crate::prefs;
use crate::ui::{EmptyState, ErrorBox, I, Icon, Page, Panel, Skeleton};
use actions::EventActions;
use facts::EventFacts;

const THEATER_KEY: &str = "ui.events.theater";

#[derive(Clone, PartialEq)]
enum Phase {
    Loading,
    /// Event id and its camera's display name. Field changes (e.g.
    /// protection) flow through `current` without rebuilding the page, so
    /// the player keeps its position.
    Loaded(String, String),
    NotFound,
    Failed(ApiError),
}

#[component]
pub fn EventDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id")).unwrap_or_default();
    let detail = use_query(Topic::Events, None, move || api::get_event(id()));
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);

    let current = Memo::new(move |_| detail.get().and_then(Result::ok));
    let phase = Memo::new(move |_| match detail.get() {
        None => Phase::Loading,
        Some(Ok(d)) => {
            let name = cameras.get().and_then(Result::ok)
                .and_then(|l| l.into_iter().find(|c| c.id == d.event.camera_id).map(|c| c.name))
                .unwrap_or_else(|| d.event.camera_id.clone());
            Phase::Loaded(d.event.id, name)
        }
        Some(Err(e)) if e.status == 404 => Phase::NotFound,
        Some(Err(e)) => Phase::Failed(e),
    });
    let back = move || prefs::get(BACK_KEY).unwrap_or_else(|| "/events".into());
    let theater = RwSignal::new(prefs::get_bool(THEATER_KEY, false));
    Effect::new(move || prefs::set(THEATER_KEY, &theater.get().to_string()));
    let navigate = use_navigate();

    // ← older, → newer
    let nav = navigate.clone();
    let keys = window_event_listener(ev::keydown, move |e| {
        let Some(d) = current.get_untracked() else { return };
        let target = match e.key().as_str() {
            "ArrowLeft" => d.previous,
            "ArrowRight" => d.next,
            _ => None,
        };
        if let Some(t) = target {
            nav(&format!("/events/{t}"), Default::default());
        }
    });
    on_cleanup(move || keys.remove());

    move || match phase.get() {
        Phase::Loading => view! { <Page title="Event"><Skeleton lines=6 height="3rem" /></Page> }.into_any(),
        Phase::Failed(error) => view! { <Page title="Event"><ErrorBox error /></Page> }.into_any(),
        Phase::NotFound => view! {
            <Page title="Event not found">
                <EmptyState icon=I::Activity title="This event no longer exists" text="It may have been deleted or removed by retention.">
                    <A href=back() attr:class="btn btn--secondary">"Back to events"</A>
                </EmptyState>
            </Page>
        }.into_any(),
        Phase::Loaded(_, camera_name) => {
            let Some(d) = current.get_untracked() else { return ().into_any() };
            let protected = Signal::derive(move || current.get().is_some_and(|d| d.event.protected));
            let start = d.event.start_time.with_timezone(&chrono::Local);
            let when = format!("{}, {}", start.format("%a %-d %b"), crate::format::time_hms(start));
            let (older, newer) = (d.previous.clone(), d.next.clone());
            let after_delete = {
                let navigate = navigate.clone();
                let target = newer.clone().or(older.clone()).map(|t| format!("/events/{t}")).unwrap_or_else(back);
                Callback::new(move |_| navigate(&target, Default::default()))
            };
            let e = d.event.clone();
            let (title, subtitle) = (labels::title(e.kind).to_string(), format!("{camera_name} · {when}"));
            let player = match (&d.recording, e.kind) {
                (None, EventType::CameraOffline) => view! {
                    <EmptyState icon=I::WifiOff title="No video while the camera was offline" text="See the details for the cause and how long it lasted." />
                }.into_any(),
                (None, EventType::CameraOnline) => view! {
                    <EmptyState icon=I::Wifi title="The camera reconnected" text="Live view and recording are available again." />
                }.into_any(),
                (None, EventType::Manual | EventType::Scheduled) => view! {
                    <EmptyState icon=I::VideoOff title="No video was saved for this recording" text="See the details for why it ended." />
                }.into_any(),
                _ => view! { <Player clip=Clip::for_event(&e, d.recording.as_ref(), camera_name.clone()) theater /> }.into_any(),
            };
            let facts = view! { <EventFacts event=e.clone() recording=d.recording.clone() camera_name=camera_name.clone() /> };
            let actions = view! { <EventActions event_id=e.id.clone() camera_id=e.camera_id.clone() protected on_deleted=after_delete /> };
            view! {
                <Page
                    title
                    subtitle
                    actions=move || view! {
                        <A href=back() attr:class="btn btn--ghost btn--sm"><Icon icon=I::ArrowLeft class="icon icon--sm" />"Events"</A>
                        <NavButton target=older.clone() label="Older" icon=I::ChevronLeft />
                        <NavButton target=newer.clone() label="Newer" icon=I::ChevronRight />
                    }
                >
                    <div class="event-layout" class:event-layout--theater=theater>
                        <div class="event-layout__main">
                            {player}
                        </div>
                        <aside class="event-layout__side">
                            <Panel title="Details">
                                {facts}
                            </Panel>
                            <Panel title="Actions">
                                {actions}
                            </Panel>
                        </aside>
                    </div>
                </Page>
            }.into_any()
        }
    }
}

#[component]
fn NavButton(target: Option<String>, label: &'static str, icon: I) -> impl IntoView {
    match target {
        Some(t) => view! {
            <A href=format!("/events/{t}") attr:class="btn btn--secondary btn--sm" attr:title=format!("{label} event (arrow key)")>
                {(icon == I::ChevronLeft).then(|| view! { <Icon icon class="icon icon--sm" /> })}
                {label}
                {(icon == I::ChevronRight).then(|| view! { <Icon icon class="icon icon--sm" /> })}
            </A>
        }.into_any(),
        None => view! { <button class="btn btn--secondary btn--sm" disabled=true>{label}</button> }.into_any(),
    }
}
