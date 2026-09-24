use leptos::prelude::*;
use leptos_router::components::A;

use crate::ui::{I, Icon, Popover};

/// Account menu. Sign-out activates once authentication exists.
#[component]
pub fn UserMenu() -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <div class="popover-anchor">
            <button
                class="icon-btn"
                class:icon-btn--active=open
                aria-label="User menu"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <Icon icon=I::CircleUser class="icon icon--lg" />
            </button>
            <Popover open class="user-menu">
                <div class="user-menu__who">
                    <div class="user-menu__name">"admin"</div>
                    <div class="user-menu__note">"Authentication disabled"</div>
                </div>
                <div class="menu">
                    <A href="/settings" attr:class="menu__item" on:click=move |_| open.set(false)>
                        <Icon icon=I::Settings class="icon icon--sm" />
                        "Settings"
                    </A>
                    <button class="menu__item" disabled title="Available once authentication is enabled">
                        <Icon icon=I::LogOut class="icon icon--sm" />
                        "Sign out"
                    </button>
                </div>
            </Popover>
        </div>
    }
}
