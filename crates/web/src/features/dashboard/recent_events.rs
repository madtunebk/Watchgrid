use leptos::prelude::*;

use crate::api::{ApiResult, Camera, Event};
use crate::features::events::EventRow;
use crate::ui::{EmptyState, I, Skeleton, async_view};

const LIMIT: usize = 8;

#[component]
pub fn RecentEvents(events: LocalResource<ApiResult<Vec<Event>>>, cameras: LocalResource<ApiResult<Vec<Camera>>>) -> impl IntoView {
    async_view(events, || view! { <Skeleton lines=5 /> }.into_any(), move |list| {
        if list.is_empty() {
            return view! { <EmptyState icon=I::Activity title="No events yet today" compact=true /> }.into_any();
        }
        let cams = cameras.get().and_then(Result::ok).unwrap_or_default();
        let name = |id: &str| cams.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_else(|| id.into());
        view! {
            <div class="event-list">
                {list.into_iter().take(LIMIT).map(|event| {
                    let camera_name = name(&event.camera_id);
                    view! { <EventRow event camera_name /> }
                }).collect_view()}
            </div>
        }
        .into_any()
    })
}
