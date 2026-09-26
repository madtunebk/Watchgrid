use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, Settings, Topic};
use crate::features::settings::save::save;
use crate::ui::form::{Field, FormSection, Switch, TextInput};
use crate::ui::{SaveBar, SaveState};

#[component]
pub fn NotificationsSection(settings: Signal<Settings>) -> impl IntoView {
    let n = settings.get_untracked().notifications;
    let offline = RwSignal::new(n.camera_offline);
    let person = RwSignal::new(n.person_detected);
    let vehicle = RwSignal::new(n.vehicle_detected);
    let storage = RwSignal::new(n.storage_low);
    let failed = RwSignal::new(n.recording_failed);
    let security = RwSignal::new(n.camera_security);
    let webhook = RwSignal::new(n.webhook_url.unwrap_or_default());
    let state = SaveState::new();

    let draft = move || (offline.get(), person.get(), vehicle.get(), storage.get(), failed.get(), security.get(), webhook.get().trim().to_string());
    let dirty = Signal::derive(move || {
        let n = settings.get().notifications;
        draft() != (n.camera_offline, n.person_detected, n.vehicle_detected, n.storage_low, n.recording_failed, n.camera_security, n.webhook_url.unwrap_or_default())
    });
    let on_save = Callback::new(move |_| {
        let (a, b, c, d, e, f, w) = draft();
        save(state, &settings.get_untracked(), |s| {
            let n = &mut s.notifications;
            (n.camera_offline, n.person_detected, n.vehicle_detected, n.storage_low, n.recording_failed, n.camera_security) = (a, b, c, d, e, f);
            n.webhook_url = (!w.is_empty()).then_some(w);
        });
    });
    let on_revert = Callback::new(move |_| {
        let n = settings.get_untracked().notifications;
        offline.set(n.camera_offline);
        person.set(n.person_detected);
        vehicle.set(n.vehicle_detected);
        storage.set(n.storage_low);
        failed.set(n.recording_failed);
        security.set(n.camera_security);
        webhook.set(n.webhook_url.unwrap_or_default());
    });

    view! {
        <div class="settings-tab">
            <FormSection title="Notify me when" description="Shown in the bell menu and sent to the webhook below.">
                <Switch checked=offline label="A camera goes offline" />
                <Switch checked=person label="A person is detected" />
                <Switch checked=vehicle label="A vehicle is detected" />
                <Switch checked=storage label="Storage is running low" />
                <Switch checked=failed label="A recording fails" description="Disk errors, stream drops during a clip." />
                <Switch checked=security label="Someone fails to sign in to a camera"
                    description="Reported by cameras with ONVIF events (wrong password on the camera itself)." />
            </FormSection>
            <FormSection title="Webhook" description="Send each notification as JSON to another system (Home Assistant, ntfy, Gotify…).">
                <Field label="Webhook URL" optional=true>
                    <TextInput value=webhook placeholder="https://homeassistant.local/api/webhook/watchgrid" mono=true />
                </Field>
            </FormSection>
            <FormSection title="Test" description="Send one notification now: it appears in the bell and goes to the saved webhook.">
                <TestButton />
            </FormSection>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}

/// "Send test notification" and what came of it.
#[component]
fn TestButton() -> impl IntoView {
    let busy = RwSignal::new(false);
    let result = RwSignal::new(None::<(bool, String)>);
    let send = move |_| {
        busy.set(true);
        result.set(None);
        spawn_local(async move {
            result.set(Some(match api::send_test_notification().await {
                Ok(r) => {
                    api::invalidate(Topic::Notifications);
                    match (r.webhook, r.webhook_ok) {
                        (None, _) => (true, "Sent. Check the bell. (No webhook is saved.)".into()),
                        (Some(status), true) => (true, format!("Sent. Check the bell; the webhook accepted it ({status}).")),
                        (Some(error), false) => (false, format!("In the bell, but the webhook failed: {error}.")),
                    }
                }
                Err(e) => (false, e.to_string()),
            }));
            busy.set(false);
        });
    };
    view! {
        <div class="test-notify">
            <button class="btn btn--secondary" disabled=busy on:click=send>
                {move || if busy.get() { "Sending…" } else { "Send test notification" }}
            </button>
            {move || result.get().map(|(ok, text)| view! { <p class="test-notify__result" class:test-notify__result--error=!ok>{text}</p> })}
        </div>
    }
}
