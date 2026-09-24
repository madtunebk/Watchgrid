use std::time::Duration;

use leptos::prelude::*;
use leptos_router::components::A;

use super::camera_overview::CameraOverview;
use super::recent_events::RecentEvents;
use super::stats::StatsRow;
use crate::api::{self, EventQuery, Topic, use_query};
use crate::clock::start_of_today;
use crate::features::cameras::NoCameras;
use crate::features::storage::StorageSummary;
use crate::features::system::{CapacitySummary, SystemSummary};
use crate::ui::{I, Icon, Page, Panel, Skeleton, async_view};

/// Owns the dashboard's queries and hands data to the sections.
#[component]
pub fn DashboardPage() -> impl IntoView {
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    let events = use_query(Topic::Events, Some(Duration::from_secs(30)), || {
        let query = EventQuery { from: Some(start_of_today()), ..Default::default() };
        async move { api::get_events(query).await.map(|page| page.events) }
    });
    let storage = use_query(Topic::Storage, Some(Duration::from_secs(60)), api::get_storage_status);
    let system = use_query(Topic::System, Some(Duration::from_secs(5)), api::get_system_status);
    // Capacity depends on camera settings, so it refreshes with them.
    let capacity = use_query(Topic::Cameras, None, api::get_capacity);

    // Only re-layout when the "has cameras" answer changes, not on every refetch.
    let has_cameras = Memo::new(move |_| cameras.get().and_then(Result::ok).map(|c| !c.is_empty()));

    let side = move || {
        view! {
            <Panel title="System" link=("Details", "/system")>
                {async_view(system, || view! { <Skeleton lines=3 /> }.into_any(), |status| view! { <SystemSummary status /> })}
            </Panel>
            <Panel title="Capacity" link=("Details", "/system")>
                {async_view(capacity, || view! { <Skeleton lines=4 /> }.into_any(), |estimate| view! { <CapacitySummary estimate /> })}
            </Panel>
            <Panel title="Storage" link=("Manage", "/storage")>
                {async_view(storage, || view! { <Skeleton lines=3 /> }.into_any(), move |status| {
                    let cameras = cameras.get().and_then(Result::ok).unwrap_or_default();
                    view! { <StorageSummary status cameras /> }
                })}
            </Panel>
        }
    };

    view! {
        <Page
            title="Dashboard"
            actions=|| view! {
                <A href="/cameras/new" attr:class="btn btn--primary btn--sm">
                    <Icon icon=I::Plus class="icon icon--sm" />"Add camera"
                </A>
            }
        >
            {move || match has_cameras.get() {
                Some(false) => view! {
                    <div class="dash">
                        <div class="dash__main"><NoCameras /></div>
                        <aside class="dash__side">{side()}</aside>
                    </div>
                }.into_any(),
                _ => view! {
                    <StatsRow cameras events storage system />
                    <div class="dash">
                        <div class="dash__main"><CameraOverview cameras /></div>
                        <aside class="dash__side">
                            <Panel title="Today's events" link=("All events", "/events")>
                                <RecentEvents events cameras />
                            </Panel>
                            {side()}
                        </aside>
                    </div>
                }.into_any(),
            }}
        </Page>
    }
}
