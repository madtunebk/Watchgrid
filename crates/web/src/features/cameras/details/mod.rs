//! Camera details: `/cameras/:id/:tab`. The page keeps the camera live
//! (state badges, record button) while settings tabs edit their own drafts.

mod advanced;
mod events;
mod live;
mod motion;
mod recording;
mod save;
mod storage;
mod stream;
mod zones;

use std::time::Duration;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::{self, CameraStatus, Topic, use_query};
use crate::features::cameras::widgets::{RecordButton, StateBadges};
use crate::ui::{EmptyState, ErrorBox, I, Icon, Page, Skeleton, Tab, TabNav};

const TABS: [(&str, &str); 7] = [
    ("live", "Live"),
    ("recording", "Recording"),
    ("motion", "Motion"),
    ("events", "Events"),
    ("stream", "Stream"),
    ("storage", "Storage"),
    ("advanced", "Advanced"),
];

#[component]
pub fn CameraDetailsPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id")).unwrap_or_default();
    let tab = Memo::new(move |_| {
        let t = params.with(|p| p.get("tab")).unwrap_or_default();
        TABS.iter().map(|(k, _)| *k).find(|k| *k == t).unwrap_or("live")
    });

    // Poll so live state (motion, recording, connection) stays current.
    let camera = use_query(Topic::Cameras, Some(Duration::from_secs(5)), move || api::get_camera(id()));
    let current = Memo::new(move |_| camera.get().and_then(Result::ok));

    // Only these transitions re-render the page; data refreshes flow into
    // the loaded view through `current` without rebuilding it.
    let phase = Memo::new(move |_| match (current.get(), camera.get()) {
        (Some(c), _) => Phase::Loaded(c.id),
        (None, Some(Err(e))) if e.status == 404 => Phase::NotFound,
        (None, Some(Err(e))) => Phase::Failed(e),
        (None, _) => Phase::Loading,
    });

    move || match phase.get() {
        Phase::Loaded(_) => view! { <Loaded current tab /> }.into_any(),
        Phase::NotFound => view! {
            <Page title="Camera not found">
                <EmptyState icon=I::VideoOff title="This camera does not exist" text="It may have been deleted.">
                    <A href="/cameras" attr:class="btn btn--secondary">"Back to cameras"</A>
                </EmptyState>
            </Page>
        }.into_any(),
        Phase::Failed(error) => view! { <Page title="Camera"><ErrorBox error /></Page> }.into_any(),
        Phase::Loading => view! { <Page title="Camera"><Skeleton lines=6 height="3rem" /></Page> }.into_any(),
    }
}

#[derive(Clone, PartialEq)]
enum Phase {
    Loading,
    Loaded(String),
    NotFound,
    Failed(api::ApiError),
}

#[component]
fn Loaded(current: Memo<Option<api::Camera>>, tab: Memo<&'static str>) -> impl IntoView {
    let initial = current.get_untracked().expect("rendered only once loaded");
    // Keep the last known camera so a transient refetch error doesn't blank the page.
    let camera = RwSignal::new(initial.clone());
    Effect::new(move || {
        if let Some(c) = current.get() {
            camera.set(c);
        }
    });
    let camera: Signal<api::Camera> = camera.into();
    let base = format!("/cameras/{}", initial.id);
    let tabs: Vec<Tab> = TABS.iter().map(|(key, label)| Tab { key, label, href: format!("{base}/{key}") }).collect();

    let title = Signal::derive(move || camera.get().name);
    let subtitle = Signal::derive(move || {
        let c = camera.get();
        if c.location.is_empty() { c.host } else { format!("{} · {}", c.host, c.location) }
    });

    view! {
        <Page
            title
            subtitle
            actions=move || {
                let c = camera.get();
                let available = c.enabled && c.status == CameraStatus::Online;
                view! {
                    <StateBadges camera=c.clone() />
                    <RecordButton camera_id=c.id.clone() recording=c.recording_active reason=c.recording_reason available />
                    <A href=format!("/cameras/{}/edit", c.id) attr:class="btn btn--secondary btn--sm">
                        <Icon icon=I::Pencil class="icon icon--sm" />"Edit"
                    </A>
                }
            }
        >
            <TabNav tabs active=tab />
            <div class="tab-panel">
                {move || match tab.get() {
                    "recording" => view! { <recording::RecordingTab camera /> }.into_any(),
                    "motion" => view! { <motion::MotionTab camera /> }.into_any(),
                    "events" => view! { <events::EventsTab camera /> }.into_any(),
                    "stream" => view! { <stream::StreamTab camera /> }.into_any(),
                    "storage" => view! { <storage::StorageTab camera /> }.into_any(),
                    "advanced" => view! { <advanced::AdvancedTab camera /> }.into_any(),
                    _ => view! { <live::LiveTab camera /> }.into_any(),
                }}
            </div>
        </Page>
    }
}
