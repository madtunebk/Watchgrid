use leptos::prelude::*;

use crate::api::EventType;
use crate::features::events::labels;
use crate::ui::Icon;

/// Event thumbnail: a small JPEG of the event's moment (made once by the
/// server), over a tinted icon that shows while it loads or if it can't.
#[component]
pub fn EventThumb(kind: EventType, #[prop(default = None)] src: Option<String>) -> impl IntoView {
    let icon = move || {
        view! {
            <span class=format!("evt-thumb evt-thumb--{}", labels::css(kind)) aria-hidden="true">
                <Icon icon=labels::icon(kind) class="icon" />
            </span>
        }
    };
    let Some(src) = src else { return icon().into_any() };
    let failed = RwSignal::new(false);
    view! {
        <span class="evt-thumb evt-thumb--clip">
            {icon()}
            <Show when=move || !failed.get()>
                <img src=src.clone() alt="" loading="lazy" decoding="async" on:error=move |_| failed.set(true) />
            </Show>
        </span>
    }
    .into_any()
}
