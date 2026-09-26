//! The sign-in form and the "no users yet" notice.

use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, AuthState, set_auth_state};
use crate::shell::Logo;

#[component]
pub fn LoginPage() -> impl IntoView {
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (u, p) = (username.get_untracked(), password.get_untracked());
        spawn_local(async move {
            match api::login(u, p).await {
                Ok(user) => {
                    password.set(String::new());
                    set_auth_state(AuthState::SignedIn(user));
                }
                Err(e) if e.code == "no_users" => set_auth_state(AuthState::NoUsers),
                Err(e) => {
                    error.set(Some(e.message));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <div class="auth-screen">
            <form class="auth-card" on:submit=submit>
                <div class="auth-card__brand"><Logo /><span>"Watchgrid"</span></div>
                <label class="field">
                    <span class="field__label">"Username"</span>
                    <input class="input" name="username" autocomplete="username" autocapitalize="off" spellcheck="false" required=true
                        prop:value=username on:input=move |ev| username.set(event_target_value(&ev)) />
                </label>
                <label class="field">
                    <span class="field__label">"Password"</span>
                    <input class="input" type="password" name="password" autocomplete="current-password" required=true
                        prop:value=password on:input=move |ev| password.set(event_target_value(&ev)) />
                </label>
                {move || error.get().map(|e| view! { <p class="auth-card__error" role="alert">{e}</p> })}
                <button class="btn btn--primary auth-card__submit" type="submit" disabled=busy>
                    {move || if busy.get() { "Signing in…" } else { "Sign in" }}
                </button>
                <p class="auth-card__note">"Accounts are managed on the server with " <code>"watchgrid user"</code> "."</p>
            </form>
        </div>
    }
}

#[component]
pub fn NoUsersPage() -> impl IntoView {
    view! {
        <div class="auth-screen">
            <div class="auth-card">
                <div class="auth-card__brand"><Logo /><span>"Watchgrid"</span></div>
                <p class="auth-card__title">"No Watchgrid users exist."</p>
                <p>"Create one from the server:"</p>
                <pre class="auth-card__cmd">"sudo watchgrid user create <username>"</pre>
                <button class="btn btn--secondary" on:click=move |_| set_auth_state(AuthState::SignedOut)>"I created one — sign in"</button>
            </div>
        </div>
    }
}
