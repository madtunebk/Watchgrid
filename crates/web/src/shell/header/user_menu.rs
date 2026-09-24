use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::{self, AuthState, Role, auth_state, set_auth_state};
use crate::ui::{I, Icon, Popover};

/// Account menu: who is signed in, and sign-out.
#[component]
pub fn UserMenu() -> impl IntoView {
    let open = RwSignal::new(false);
    let state: Signal<AuthState> = auth_state().into();
    let who = Signal::derive(move || match state.get() {
        AuthState::SignedIn(u) => u.username,
        _ => String::new(),
    });
    let role = Signal::derive(move || match state.get() {
        AuthState::SignedIn(u) if u.role == Role::Admin => "Administrator",
        AuthState::SignedIn(_) => "Viewer (read-only)",
        _ => "",
    });
    let sign_out = move |_| {
        open.set(false);
        spawn_local(async move {
            let _ = api::logout().await;
            set_auth_state(AuthState::SignedOut);
        });
    };
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
                    <div class="user-menu__name">{who}</div>
                    <div class="user-menu__note">{role}</div>
                </div>
                <div class="menu">
                    <A href="/settings" attr:class="menu__item" on:click=move |_| open.set(false)>
                        <Icon icon=I::Settings class="icon icon--sm" />
                        "Settings"
                    </A>
                    <button class="menu__item" on:click=sign_out>
                        <Icon icon=I::LogOut class="icon icon--sm" />
                        "Sign out"
                    </button>
                </div>
            </Popover>
        </div>
    }
}
