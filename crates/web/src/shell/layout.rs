use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;

use super::header::Header;
use super::sidebar::Sidebar;
use crate::prefs;

const COLLAPSED_KEY: &str = "ui.sidebarCollapsed";

#[component]
pub fn Shell() -> impl IntoView {
    // Without an explicit choice, start collapsed on narrower screens so the
    // content keeps its room; the user's toggle is remembered afterwards.
    let narrow = web_sys::window().and_then(|w| w.inner_width().ok()).and_then(|v| v.as_f64()).is_some_and(|w| w < 1400.0);
    let collapsed = RwSignal::new(prefs::get_bool(COLLAPSED_KEY, narrow));
    Effect::new(move || prefs::set(COLLAPSED_KEY, &collapsed.get().to_string()));

    // The mobile drawer is open only on the path it was opened from, so any
    // navigation closes it without extra wiring.
    let pathname = use_location().pathname;
    let drawer_path = RwSignal::new(None::<String>);
    let mobile_open = Signal::derive(move || drawer_path.get().is_some_and(|p| p == pathname.get()));

    view! {
        <div class="shell">
            <Sidebar collapsed mobile_open on_close=move || drawer_path.set(None) />
            <div class="shell__main">
                <Header on_open_menu=move || drawer_path.set(Some(pathname.get_untracked())) />
                <main class="shell__content">
                    <Outlet />
                </main>
            </div>
        </div>
    }
}
