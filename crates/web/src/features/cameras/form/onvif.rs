use leptos::prelude::*;

use super::draft::Draft;
use super::probe::{self, Probe};
use super::validate::Errors;
use crate::api::{self, OnvifConfig, OnvifProbe};
use crate::ui::form::{Field, FormSection, Switch, TextInput};
use crate::ui::{I, Icon};

/// Optional ONVIF: lets the camera report motion / person / vehicle events itself.
#[component]
pub fn OnvifSection(draft: Draft, errors: Signal<Errors>, editing: bool) -> impl IntoView {
    let state = RwSignal::new(Probe::<OnvifProbe>::Idle);

    // Pre-fill the standard device-service URL and the camera login when enabled.
    Effect::new(move |was_on: Option<bool>| {
        let on = draft.onvif_enabled.get();
        if on && was_on == Some(false) {
            if draft.onvif_url.get_untracked().is_empty() && !draft.host.get_untracked().is_empty() {
                draft.onvif_url.set(format!("http://{}/onvif/device_service", draft.host.get_untracked().trim()));
            }
            if draft.onvif_username.get_untracked().is_empty() {
                draft.onvif_username.set(draft.username.get_untracked());
            }
        }
        on
    });

    let test = move |_| {
        let config = OnvifConfig {
            url: draft.onvif_url.get(),
            username: draft.onvif_username.get(),
            password: Some(draft.onvif_password.get()).filter(|p| !p.is_empty()),
        };
        probe::run(state, api::test_onvif(config));
    };

    view! {
        <FormSection title="ONVIF events" description="Optional. Uses the camera's own motion and object detection.">
            <Switch checked=draft.onvif_enabled label="Receive events from the camera (ONVIF)"
                description="Without ONVIF, Watchgrid detects motion itself from the video." />
            <Show when=move || draft.onvif_enabled.get()>
                <div class="form-grid">
                    <div class="form-grid__wide">
                        <Field label="ONVIF URL" error=Signal::derive(move || errors.get().onvif_url)>
                            <TextInput value=draft.onvif_url placeholder="http://192.168.1.26/onvif/device_service" mono=true />
                        </Field>
                    </div>
                    <Field label="ONVIF username">
                        <TextInput value=draft.onvif_username placeholder="admin" />
                    </Field>
                    <Field label="ONVIF password" hint=if editing { "Leave empty to keep the current password" } else { "" }>
                        <TextInput value=draft.onvif_password kind="password" autocomplete="new-password" />
                    </Field>
                </div>
                <div class="test-row">
                    <button type="button" class="btn btn--secondary btn--sm" on:click=test disabled=move || state.get() == Probe::Running>
                        <Icon icon=I::Activity class="icon icon--sm" />"Test ONVIF"
                    </button>
                    {move || probe::onvif_view(state.get())}
                </div>
            </Show>
        </FormSection>
    }
}
