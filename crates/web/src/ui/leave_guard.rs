//! Asks before leaving a form with unsaved changes: closing or reloading
//! the tab (the browser's own prompt) or following a link inside the app.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

const QUESTION: &str = "You have unsaved changes. Leave without saving them?";

/// Guard the page while `dirty` is true; removed with the component.
pub fn guard_unsaved(dirty: Signal<bool>) {
    let Some(window) = web_sys::window() else { return };
    let unload = Closure::<dyn Fn(web_sys::BeforeUnloadEvent)>::new(move |e: web_sys::BeforeUnloadEvent| {
        if dirty.get_untracked() {
            e.prevent_default();
            e.set_return_value(QUESTION);
        }
    });
    // Capture phase: runs before the router turns the click into navigation.
    let click = Closure::<dyn Fn(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
        if !dirty.get_untracked() || e.default_prevented() || e.button() != 0 || e.ctrl_key() || e.meta_key() || e.shift_key() || e.alt_key() {
            return;
        }
        let Some(link) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).and_then(|el| el.closest("a[href]").ok().flatten()) else { return };
        if link.get_attribute("target").is_some_and(|t| t == "_blank") || link.has_attribute("download") {
            return;
        }
        let leave = web_sys::window().and_then(|w| w.confirm_with_message(QUESTION).ok()).unwrap_or(true);
        if !leave {
            e.prevent_default();
            e.stop_immediate_propagation();
        }
    });
    let unload_fn: js_sys::Function = unload.as_ref().unchecked_ref::<js_sys::Function>().clone();
    let click_fn: js_sys::Function = click.as_ref().unchecked_ref::<js_sys::Function>().clone();
    let _ = window.add_event_listener_with_callback("beforeunload", &unload_fn);
    let _ = window.add_event_listener_with_callback_and_bool("click", &click_fn, true);
    // The closures live as long as the listeners (removed below).
    unload.forget();
    click.forget();
    let listeners = StoredValue::new_local((unload_fn, click_fn));
    on_cleanup(move || {
        let Some(window) = web_sys::window() else { return };
        listeners.with_value(|(unload, click)| {
            let _ = window.remove_event_listener_with_callback("beforeunload", unload);
            let _ = window.remove_event_listener_with_callback_and_bool("click", click, true);
        });
    });
}
