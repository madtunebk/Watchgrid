use leptos::prelude::*;

use crate::api::Camera;
use crate::ui::{I, Icon};

/// Placeholder for an unused tile: pick a camera to show here.
#[component]
pub fn EmptySlot(#[prop(into)] available: Signal<Vec<Camera>>, on_pick: Callback<String>) -> impl IntoView {
    view! {
        <div class="tile tile--empty">
            <Icon icon=I::Plus class="icon icon--lg" />
            {move || {
                let list = available.get();
                if list.is_empty() {
                    return view! { <span class="tile__hint">"All cameras are shown"</span> }.into_any();
                }
                view! {
                    <select class="tile__select" aria-label="Show camera in this tile"
                        on:change=move |ev| {
                            let id = event_target_value(&ev);
                            if !id.is_empty() {
                                on_pick.run(id);
                            }
                        }>
                        <option value="" selected>"Add camera…"</option>
                        {list.into_iter().map(|c| view! { <option value=c.id.clone()>{c.name}</option> }).collect_view()}
                    </select>
                }.into_any()
            }}
            <span class="tile__hint">"or drag a camera here"</span>
        </div>
    }
}
