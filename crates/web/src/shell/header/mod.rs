//! Top bar: breadcrumbs, live NVR status indicators, notifications, user menu.

mod breadcrumbs;
mod notifications;
mod status;
mod user_menu;

use leptos::prelude::*;

use crate::ui::{I, Icon};

#[component]
pub fn Header(on_open_menu: impl Fn() + Send + Sync + 'static) -> impl IntoView {
    view! {
        <header class="header">
            <button class="icon-btn only-mobile" aria-label="Open menu" on:click=move |_| on_open_menu()>
                <Icon icon=I::Menu class="icon icon--lg" />
            </button>
            <breadcrumbs::Breadcrumbs />
            <div class="header__right">
                <status::ServerStatus />
                <status::RecordingCount />
                <status::ConnectionIndicator />
                <status::Clock />
                <div class="header__divider"></div>
                <crate::features::arm::ArmMenu />
                <notifications::Notifications />
                <user_menu::UserMenu />
            </div>
        </header>
    }
}
