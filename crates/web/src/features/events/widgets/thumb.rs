use leptos::prelude::*;

use crate::api::EventType;
use crate::features::events::labels;
use crate::ui::Icon;

/// Event thumbnail. Until the recorder stores real snapshots this is a
/// tinted placeholder with the event icon; `src` takes over when present.
#[component]
pub fn EventThumb(kind: EventType, #[prop(default = None)] src: Option<String>) -> impl IntoView {
    match src {
        Some(src) => view! { <img class="evt-thumb" src=src alt="" loading="lazy" /> }.into_any(),
        None => view! {
            <span class=format!("evt-thumb evt-thumb--{}", labels::css(kind)) aria-hidden="true">
                <Icon icon=labels::icon(kind) class="icon" />
            </span>
        }
        .into_any(),
    }
}
