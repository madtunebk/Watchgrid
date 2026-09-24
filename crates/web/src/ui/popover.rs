use leptos::ev;
use leptos::prelude::*;

/// Floating panel anchored to its parent (`.popover-anchor`). Closes on
/// outside click (via a transparent backdrop) and on Escape.
#[component]
pub fn Popover(open: RwSignal<bool>, #[prop(default = "")] class: &'static str, children: ChildrenFn) -> impl IntoView {
    let esc = window_event_listener(ev::keydown, move |e| {
        if e.key() == "Escape" {
            open.set(false);
        }
    });
    on_cleanup(move || esc.remove());

    view! {
        <Show when=move || open.get()>
            <div class="popover-backdrop" on:click=move |_| open.set(false)></div>
            <div class=format!("popover {class}")>{children()}</div>
        </Show>
    }
}
