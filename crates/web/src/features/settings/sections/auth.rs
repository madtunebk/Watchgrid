use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, Role, Settings, Topic, invalidate, use_query};
use crate::features::settings::save::save;
use crate::format;
use crate::ui::form::{Field, FormSection, NumberInput};
use crate::ui::{follow_server, Badge, SaveBar, SaveState, Skeleton, Tone, async_view};

#[component]
pub fn AuthSection(settings: Signal<Settings>) -> impl IntoView {
    let timeout = RwSignal::new(settings.get_untracked().auth.session_timeout_minutes);
    let state = SaveState::new();
    let dirty = Signal::derive(move || timeout.get() != settings.get().auth.session_timeout_minutes);
    let on_save = Callback::new(move |_| {
        let t = timeout.get_untracked();
        save(state, &settings.get_untracked(), move |s| s.auth.session_timeout_minutes = t);
    });
    let on_revert = Callback::new(move |_| timeout.set(settings.get_untracked().auth.session_timeout_minutes));
    follow_server(move || timeout.get(), move || settings.get().auth.session_timeout_minutes, on_revert);

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
            <FormSection title="Sign-in" description="Sign-in is always required for the web interface and the API.">
                <Field label="Session timeout" hint="Signed-in browsers stay logged in this long without activity (up to 7 days).">
                    <NumberInput value=timeout min=5 max=10_080 suffix="minutes" />
                </Field>
            </FormSection>
            <SaveBar state dirty on_save on_revert />

            <FormSection title="Users" description="Admins can change settings; viewers can only watch. Accounts are managed on the server, never from the browser.">
                {async_view(users, || view! { <Skeleton lines=2 /> }.into_any(), |list| view! {
                    <div class="table-wrap">
                        <table class="table">
                            <thead><tr><th>"User"</th><th>"Role"</th><th>"Last sign-in"</th></tr></thead>
                            <tbody>
                                {list.into_iter().map(|u| view! {
                                    <tr>
                                        <td class="table__primary">{u.username}</td>
                                        <td>{match u.role {
                                            Role::Admin => view! { <Badge tone=Tone::Accent label="ADMIN" /> }.into_any(),
                                            Role::Viewer => view! { <Badge tone=Tone::Offline label="VIEWER" /> }.into_any(),
                                        }}</td>
                                        <td class="muted">{u.last_login.map(format::relative).unwrap_or_else(|| "Never".into())}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                })}
                <div class="cli-help">
                    <p class="note">"To add, remove or change accounts, run on the server:"</p>
                    <pre class="cli-help__cmds">"sudo watchgrid user create <name> [--viewer]\nsudo watchgrid user passwd <name>\nsudo watchgrid user disable|enable <name>\nsudo watchgrid user delete <name>\nsudo watchgrid user list"</pre>
                    <p class="note">"With Docker: " <code>"sudo docker exec -it watchgrid watchgrid user …"</code></p>
                </div>
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
                                            <td class="table__primary" title=s.client.clone()>{format::client(&s.client)}{s.current.then(|| view! { <span class="muted">" (you)"</span> })}</td>
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
