use leptos::html::Span;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::api::EventType;
use crate::features::events::labels;
use crate::ui::Icon;

/// Event thumbnail: the event's frame from its clip, or a tinted icon.
///
/// A clip thumbnail (`…/media#t=<seconds>`) is a muted `<video>` the
/// browser decodes itself; it only loads once scrolled into view.
#[component]
pub fn EventThumb(kind: EventType, #[prop(default = None)] src: Option<String>) -> impl IntoView {
    let placeholder = move || {
        view! {
            <span class=format!("evt-thumb evt-thumb--{}", labels::css(kind)) aria-hidden="true">
                <Icon icon=labels::icon(kind) class="icon" />
            </span>
        }
    };
    match src {
        Some(src) if src.contains("/media#t=") => view! { <ClipFrame src fallback=placeholder.into_any() /> }.into_any(),
        Some(src) => view! { <img class="evt-thumb" src=src alt="" loading="lazy" /> }.into_any(),
        None => placeholder().into_any(),
    }
}

#[component]
fn ClipFrame(src: String, fallback: AnyView) -> impl IntoView {
    let host = NodeRef::<Span>::new();
    let visible = RwSignal::new(false);
    host.on_load(move |el| {
        let on_seen = Closure::<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>::new(move |entries: js_sys::Array, observer: web_sys::IntersectionObserver| {
            let seen = entries.iter().any(|e| e.unchecked_into::<web_sys::IntersectionObserverEntry>().is_intersecting());
            if seen {
                visible.set(true);
                observer.disconnect();
            }
        });
        let options = web_sys::IntersectionObserverInit::new();
        options.set_root_margin("200px");
        if let Ok(observer) = web_sys::IntersectionObserver::new_with_options(on_seen.as_ref().unchecked_ref(), &options) {
            observer.observe(&el);
        }
        // Lives as long as the observer needs it (it disconnects after the first sighting).
        on_seen.forget();
    });
    view! {
        <span class="evt-thumb evt-thumb--clip" node_ref=host>
            {move || if visible.get() {
                view! { <video src=src.clone() muted=true preload="metadata" playsinline=true aria-hidden="true"></video> }.into_any()
            } else {
                ().into_any()
            }}
            {fallback}
        </span>
    }
}
