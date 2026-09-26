use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::Event;
use crate::features::events::widgets::{EventChip, EventThumb};
use crate::format;
use crate::ui::{I, Icon, SelectCell};

/// Full-width row on the Events page, with its selection checkbox.
#[component]
pub fn EventListRow(event: Event, camera_name: String, selected: RwSignal<BTreeSet<String>>) -> impl IntoView {
    let is_selected = {
        let id = event.id.clone();
        Memo::new(move |_| selected.with(|s| s.contains(&id)))
    };
    let live = event.end_time.is_none();
    let time = crate::format::time_hms(event.start_time.with_timezone(&chrono::Local));
    let detection = event.detections.first().map(|d| format!("{} {:.0}%", d.label, d.confidence * 100.0));

    view! {
        <div class="select-row" class:select-row--selected=is_selected>
        <SelectCell id=event.id.clone() selected label="Select event" />
        <A href=format!("/events/{}", event.id) attr:class="evt-row">
            <EventThumb kind=event.kind src=event.thumbnail.clone() />
            <span class="evt-row__time">{time}</span>
            <EventChip kind=event.kind />
            <span class="evt-row__camera truncate">{camera_name}</span>
            <span class="evt-row__detection truncate">{detection.unwrap_or_default()}</span>
            <span class="evt-row__flags">
                {event.protected.then(|| view! { <span title="Protected" class="evt-row__lock"><Icon icon=I::Lock class="icon icon--sm" /></span> })}
            </span>
            <span class="evt-row__duration" class:evt-row__duration--live=live>
                {if live { "LIVE".to_string() } else { format::duration(event.duration) }}
            </span>
            <Icon icon=I::ChevronRight class="icon icon--sm evt-row__chevron" />
        </A>
        </div>
    }
}
