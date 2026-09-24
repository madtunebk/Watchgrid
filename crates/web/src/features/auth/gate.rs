//! Shows the app only to a signed-in user; otherwise the sign-in page (or
//! the "create a user on the server" notice when no accounts exist).

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::login::{LoginPage, NoUsersPage};
use crate::api::{self, AuthState, auth_state, provide_connection, set_auth_state};
use crate::ui::{I, Icon};

#[component]
pub fn AuthGate(children: ChildrenFn) -> impl IntoView {
    let state = auth_state();
    // Find out who we are once at start-up.
    spawn_local(async move {
        let next = match api::current_user().await {
            Ok(user) => AuthState::SignedIn(user),
            Err(e) if e.code == "no_users" => AuthState::NoUsers,
            Err(e) if e.status == 401 => AuthState::SignedOut,
            // Server unreachable etc.: let the app show its own error states.
            Err(_) => AuthState::SignedOut,
        };
        set_auth_state(next);
    });

    move || match state.get() {
        AuthState::Checking => view! {
            <div class="auth-screen"><Icon icon=I::Loader class="icon icon--xl spin" /></div>
        }.into_any(),
        AuthState::SignedOut => view! { <LoginPage /> }.into_any(),
        AuthState::NoUsers => view! { <NoUsersPage /> }.into_any(),
        AuthState::SignedIn(_) => {
            // Live updates need a session, so they start only now.
            provide_connection();
            children().into_any()
        }
    }
}
