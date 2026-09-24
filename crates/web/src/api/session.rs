//! Who is signed in. The HTTP client reports expired sessions here, and the
//! auth gate re-renders the app accordingly.

use leptos::prelude::*;
use watchgrid_model::User;

#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    Checking,
    SignedOut,
    /// No accounts exist yet; one must be created from the server CLI.
    NoUsers,
    SignedIn(User),
}

thread_local! {
    static STATE: ArcRwSignal<AuthState> = ArcRwSignal::new(AuthState::Checking);
}

pub fn auth_state() -> ArcRwSignal<AuthState> {
    STATE.with(Clone::clone)
}

pub fn set_auth_state(s: AuthState) {
    let signal = auth_state();
    if signal.get_untracked() != s {
        signal.set(s);
    }
}

/// Any request rejected for a missing/expired session lands here.
pub fn session_expired() {
    if matches!(auth_state().get_untracked(), AuthState::SignedIn(_) | AuthState::Checking) {
        set_auth_state(AuthState::SignedOut);
    }
}
