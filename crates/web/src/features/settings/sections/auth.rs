use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, Role, Settings, Topic, invalidate, use_query};
use crate::features::settings::save::save;
use crate::format;
use crate::ui::form::{Field, FormSection, NumberInput, Switch};
use crate::ui::{Badge, SaveBar, SaveState, Skeleton, Tone, async_view};

#[component]
pub fn AuthSection(settings: Signal<Settings>) -> impl IntoView {
    let a = settings.get_untracked().auth;
    let enabled = RwSignal::new(a.enabled);
    let timeout = RwSignal::new(a.session_timeout_minutes);
    let state = SaveState::new();
    let dirty = Signal::derive(move || {
        let a = settings.get().auth;
        (enabled.get(), timeout.get()) != (a.enabled, a.session_timeout_minutes)
    });
    let on_save = Callback::new(move |_| {
        let (e, t) = (enabled.get_untracked(), timeout.get_untracked());
        save(state, &settings.get_untracked(), |s| {
            s.auth.enabled = e;
            s.auth.session_timeout_minutes = t;
        });
    });
    let on_revert = Callback::new(move |_| {
        let a = settings.get_untracked().auth;
        enabled.set(a.enabled);
        timeout.set(a.session_timeout_minutes);
    });

    let users = use_query(Topic::Settings, None, api::get_users);
    let sessions = use_query(Topic::Settings, None, api::get_sessions);
    let revoke = move |id: String| {
        spawn_local(async move {
            if api::revoke_session(id).await.is_ok() {
                invalidate(Topic::Settings);
            }
        });
    };

    view! {
        <div class="settings-tab">
            <FormSection title="Sign-in">
                <Switch checked=enabled label="Require sign-in" description="Everyone must log in to use the web interface and API." />
                <Show when=move || enabled.get()>
                    <p class="note note--warn">"Stored now, enforced once the authentication backend is in place. Make sure the admin password is set before relying on it."</p>
                </Show>
                <Field label="Session timeout" hint="Signed-in browsers stay logged in this long without activity.">
                    <NumberInput value=timeout min=5 max=43_200 suffix="minutes" />
                </Field>
            </FormSection>
            <SaveBar state dirty on_save on_revert />

            <FormSection title="Users" description="Admins can change settings; viewers can watch live video and recordings.">
                {async_view(users, || view! { <Skeleton lines=2 /> }.into_any(), |list| view! {
                    <div class="table-wrap">
                        <table class="table">
                            <thead><tr><th>"User"</th><th>"Role"</th><th>"Last sign-in"</th><th class="actions"></th></tr></thead>
                            <tbody>
                                {list.into_iter().map(|u| view! {
                                    <tr>
                                        <td class="table__primary">{u.username}</td>
                                        <td>{match u.role {
                                            Role::Admin => view! { <Badge tone=Tone::Accent label="ADMIN" /> }.into_any(),
                                            Role::Viewer => view! { <Badge tone=Tone::Offline label="VIEWER" /> }.into_any(),
                                        }}</td>
                                        <td class="muted">{u.last_login.map(format::relative).unwrap_or_else(|| "Never".into())}</td>
                                        <td class="actions"><button class="btn btn--secondary btn--sm" disabled=true title="Arrives with the authentication backend">"Reset password"</button></td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                    <button class="btn btn--secondary btn--sm" disabled=true title="Arrives with the authentication backend">"Add user"</button>
                })}
            </FormSection>

            <FormSection title="Active sessions" description="Browsers and apps currently signed in.">
                {async_view(sessions, || view! { <Skeleton lines=3 /> }.into_any(), move |list| view! {
                    <div class="table-wrap">
                        <table class="table">
                            <thead><tr><th>"Device"</th><th>"User"</th><th>"Address"</th><th>"Last seen"</th><th class="actions"></th></tr></thead>
                            <tbody>
                                {list.into_iter().map(|s| {
                                    let id = s.id.clone();
                                    view! {
                                        <tr>
                                            <td class="table__primary">{s.client}{s.current.then(|| view! { <span class="muted">" (you)"</span> })}</td>
                                            <td>{s.username}</td>
                                            <td class="mono">{s.address}</td>
                                            <td class="muted">{format::relative(s.last_seen)}</td>
                                            <td class="actions">
                                                {(!s.current).then(|| view! {
                                                    <button class="btn btn--danger btn--sm" on:click=move |_| revoke(id.clone())>"Sign out"</button>
                                                })}
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                })}
            </FormSection>
        </div>
    }
}
