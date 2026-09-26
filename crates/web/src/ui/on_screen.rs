//! Whether an element is on screen and the browser tab is showing — to
//! run live previews only while someone can see them.

use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{IntersectionObserver, IntersectionObserverEntry, IntersectionObserverInit};

/// True while `node` is within 150 px of the viewport and the tab is visible.
pub fn use_on_screen(node: NodeRef<Div>) -> Signal<bool> {
    let in_view = RwSignal::new(false);
    let tab_visible = RwSignal::new(!document().hidden());
    let observer = StoredValue::new_local(None::<IntersectionObserver>);

    node.on_load(move |el| {
        let seen = Closure::<dyn FnMut(js_sys::Array, IntersectionObserver)>::new(move |entries: js_sys::Array, _: IntersectionObserver| {
            if let Some(last) = entries.iter().last() {
                in_view.set(last.unchecked_into::<IntersectionObserverEntry>().is_intersecting());
            }
        });
        let options = IntersectionObserverInit::new();
        options.set_root_margin("150px");
        if let Ok(o) = IntersectionObserver::new_with_options(seen.as_ref().unchecked_ref(), &options) {
            o.observe(&el);
            observer.set_value(Some(o));
        }
        // Lives while the observer does (disconnected on cleanup).
        seen.forget();
    });
    let tab = window_event_listener_untyped("visibilitychange", move |_| tab_visible.set(!document().hidden()));
    on_cleanup(move || {
        tab.remove();
        if let Some(o) = observer.get_value() {
            o.disconnect();
        }
    });
    Signal::derive(move || in_view.get() && tab_visible.get())
}
