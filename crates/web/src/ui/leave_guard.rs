//! Asks before leaving a form with unsaved changes: a link inside the app
//! opens the app's own dialog; closing or reloading the tab gets the
//! browser's prompt (pages can't show their own there).

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use super::dialog::ConfirmDialog;

/// Guards the page while `dirty` is true; removed with the component.
#[component]
pub fn LeaveGuard(#[prop(into)] dirty: Signal<bool>) -> impl IntoView {
    let open = RwSignal::new(false);
    // Where the stopped link was going: an in-app path, or a full URL.
    let target = StoredValue::new(None::<(bool, String)>);
    let navigate = use_navigate();

    let unload = Closure::<dyn Fn(web_sys::BeforeUnloadEvent)>::new(move |e: web_sys::BeforeUnloadEvent| {
        if dirty.get_untracked() {
            e.prevent_default();
            e.set_return_value("unsaved");
        }
    });
    // Capture phase: runs before the router turns the click into navigation.
    let click = Closure::<dyn Fn(web_sys::MouseEvent)>::new(move |e: web_sys::MouseEvent| {
        if !dirty.get_untracked() || e.default_prevented() || e.button() != 0 || e.ctrl_key() || e.meta_key() || e.shift_key() || e.alt_key() {
            return;
        }
        let Some(link) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).and_then(|el| el.closest("a[href]").ok().flatten()) else { return };
        let Ok(link) = link.dyn_into::<web_sys::HtmlAnchorElement>() else { return };
        if link.target() == "_blank" || link.has_attribute("download") {
            return;
        }
        let same_origin = web_sys::window().and_then(|w| w.location().origin().ok()).is_some_and(|o| o == link.origin());
        let to = if same_origin { (true, format!("{}{}{}", link.pathname(), link.search(), link.hash())) } else { (false, link.href()) };
        e.prevent_default();
        e.stop_immediate_propagation();
        target.set_value(Some(to));
        open.set(true);
    });
    let unload_fn: js_sys::Function = unload.as_ref().unchecked_ref::<js_sys::Function>().clone();
    let click_fn: js_sys::Function = click.as_ref().unchecked_ref::<js_sys::Function>().clone();
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback("beforeunload", &unload_fn);
        let _ = window.add_event_listener_with_callback_and_bool("click", &click_fn, true);
    }
    // The closures live as long as the listeners (removed below).
    unload.forget();
    click.forget();
    let listeners = StoredValue::new_local((unload_fn, click_fn));
    let remove = move || {
        let Some(window) = web_sys::window() else { return };
        listeners.with_value(|(unload, click)| {
            let _ = window.remove_event_listener_with_callback("beforeunload", unload);
            let _ = window.remove_event_listener_with_callback_and_bool("click", click, true);
        });
    };
    on_cleanup(remove);

    let leave = Callback::new(move |_| {
        open.set(false);
        let Some((in_app, to)) = target.get_value() else { return };
        // Chosen: leave without asking again.
        remove();
        if in_app {
            navigate(&to, Default::default());
        } else if let Some(w) = web_sys::window() {
            let _ = w.location().set_href(&to);
        }
    });

    view! {
        <ConfirmDialog open title="Leave without saving?" confirm_label="Leave without saving" danger=true
            message="You have unsaved changes on this page. If you leave now, they are lost." on_confirm=leave />
    }
}
