use leptos::prelude::*;
use leptos_router::components::A;

use super::chip::EventChip;
use crate::api::Event;
use crate::format;

/// One line of an event list: time, type, camera, duration.
#[component]
pub fn EventRow(event: Event, camera_name: String) -> impl IntoView {
    let live = event.end_time.is_none();
    view! {
        <A href=format!("/events/{}", event.id) attr:class="event-row">
            <span class="event-row__time">{format::time_of_day(event.start_time)}</span>
            <EventChip kind=event.kind />
            <span class="event-row__camera truncate">{camera_name}</span>
            <span class="event-row__duration" class:event-row__duration--live=live>
                {if live { "LIVE".to_string() } else { format::duration(event.duration) }}
            </span>
        </A>
    }
}
