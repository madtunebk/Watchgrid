use leptos::prelude::*;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;

use super::header::Header;
use super::sidebar::Sidebar;
use crate::api::{self, Topic, use_query};
use crate::{format, prefs};

const COLLAPSED_KEY: &str = "ui.sidebarCollapsed";
/// Pages that take the whole window: no header, the sidebar as a drawer.
const IMMERSIVE: [&str; 1] = ["/live"];

/// Opens the navigation drawer (for pages without the header's menu button).
#[derive(Clone, Copy)]
pub struct OpenMenu(pub Callback<()>);

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
    let immersive = Signal::derive(move || IMMERSIVE.contains(&pathname.get().as_str()));
    provide_context(OpenMenu(Callback::new(move |_| drawer_path.set(Some(pathname.get_untracked())))));

    // Dates and times everywhere follow Settings → General.
    let settings = use_query(Topic::Settings, None, api::get_settings);
    Effect::new(move || {
        if let Some(s) = settings.get().and_then(Result::ok) {
            format::set_display(s.general.date_format, s.general.clock_24h);
        }
    });

    view! {
        <div class="shell" class:shell--immersive=immersive>
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
