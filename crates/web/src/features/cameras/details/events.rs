use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{self, Camera, EventQuery, Topic, use_query};
use crate::features::events::EventRow;
use crate::ui::{EmptyState, I, Icon, Skeleton, async_view};

const LIMIT: usize = 25;

#[component]
pub fn EventsTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let id = camera.get_untracked().id;
    let events = use_query(Topic::Events, None, move || {
        let query = EventQuery { camera_id: Some(id.clone()), limit: Some(LIMIT as u32), ..Default::default() };
        async move { api::get_events(query).await.map(|page| page.events) }
    });
    view! {
        <div class="settings-tab">
            <div class="section__head">
                <h2 class="section__title">"Recent events"</h2>
                <A href="/events" attr:class="panel__link">"Open in Events"<Icon icon=I::ArrowRight class="icon icon--sm" /></A>
            </div>
            <div class="panel"><div class="panel__body">
                {async_view(events, || view! { <Skeleton lines=6 /> }.into_any(), move |list| {
                    if list.is_empty() {
                        return view! { <EmptyState icon=I::Activity title="No events yet" compact=true
                            text="Events appear here when this camera detects motion or you record manually." /> }.into_any();
                    }
                    let name = camera.get_untracked().name;
                    view! {
                        <div class="event-list">
                            {list.into_iter().take(LIMIT).map(|event| view! { <EventRow event camera_name=name.clone() /> }).collect_view()}
                        </div>
                    }.into_any()
                })}
            </div></div>
        </div>
    }
}
