use leptos::prelude::*;

use super::draft::Draft;
use super::probe::{self, Probe};
use super::validate::Errors;
use crate::api::{self, ConnectionProbe, ConnectionTest};
use crate::ui::form::{Field, FormSection, TextInput};
use crate::ui::{I, Icon};

#[component]
pub fn ConnectionSection(draft: Draft, errors: Signal<Errors>, editing: bool, camera_id: Option<String>) -> impl IntoView {
    let state = RwSignal::new(Probe::<ConnectionProbe>::Idle);
    let test = move |_| {
        let pass = draft.password.get();
        let req = ConnectionTest {
            host: draft.host.get(),
            username: draft.username.get(),
            password: (!pass.is_empty()).then_some(pass),
            camera_id: camera_id.clone(),
        };
        probe::run(state, api::test_connection(req));
    };
    let password_hint = if editing { "Leave empty to keep the current password" } else { "" };

    view! {
        <FormSection title="Connection" description="Network address and login of the camera.">
            <div class="form-grid">
                <div class="form-grid__wide">
                    <Field label="Host / IP" required=true error=Signal::derive(move || errors.get().host)>
                        <TextInput value=draft.host placeholder="192.168.1.26" mono=true />
                    </Field>
                </div>
                <Field label="Username">
                    <TextInput value=draft.username placeholder="admin" autocomplete="off" />
                </Field>
                <Field label="Password" hint=password_hint>
                    <TextInput value=draft.password kind="password" autocomplete="new-password"
                        placeholder=if editing { "••••••••" } else { "" } />
                </Field>
            </div>
            <div class="test-row">
                <button type="button" class="btn btn--secondary btn--sm" on:click=test
                    disabled=move || state.get() == Probe::Running>
                    <Icon icon=I::Plug class="icon icon--sm" />"Test connection"
                </button>
                {move || probe::connection_view(state.get())}
            </div>
        </FormSection>
    }
}
