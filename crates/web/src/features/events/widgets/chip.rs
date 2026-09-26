use leptos::prelude::*;

use crate::features::events::labels;
use crate::api::EventType;

/// Coloured event-type tag, e.g. PERSON.
#[component]
pub fn EventChip(kind: EventType) -> impl IntoView {
    view! { <span class=format!("evt-chip evt-chip--{}", labels::css(kind))>{labels::tag(kind)}</span> }
}
