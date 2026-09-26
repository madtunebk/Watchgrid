use leptos::prelude::*;

use super::draft::Draft;
use super::probe::{self, Probe};
use super::validate::Errors;
use crate::api::{self, StreamProbe, StreamTest};
use crate::ui::form::{Field, FormSection, TextInput};
use crate::ui::{I, Icon};

#[component]
pub fn StreamsSection(draft: Draft, errors: Signal<Errors>, camera_id: Option<String>) -> impl IntoView {
    let main = RwSignal::new(Probe::<StreamProbe>::Idle);
    let sub = RwSignal::new(Probe::<StreamProbe>::Idle);

    // Suggest the usual URL shape once a host is known.
    let suggest = move |path: &str| {
        let host = draft.host.get();
        let host = host.trim();
        if host.is_empty() { format!("rtsp://192.168.1.26:554/{path}") } else { format!("rtsp://{host}:554/{path}") }
    };
    let sub_empty = move || draft.sub_url.get().trim().is_empty();
    // With no password typed, an existing camera is tested with its stored one (server-side).
    let request = move |url: String| {
        let pass = draft.password.get_untracked();
        StreamTest { url, username: draft.username.get_untracked(), password: (!pass.is_empty()).then_some(pass), camera_id: camera_id.clone() }
    };
    let request = StoredValue::new(request);

    view! {
        <FormSection title="Streams" description="The RTSP stream Watchgrid records and shows live.">
            <Field label="Main RTSP URL" required=true error=Signal::derive(move || errors.get().main_url)
                hint="Credentials from Connection are used automatically; don't put them in the URL.">
                <TextInput value=draft.main_url placeholder=Signal::derive(move || suggest("stream1")) mono=true />
            </Field>
            <div class="test-row">
                <button type="button" class="btn btn--secondary btn--sm" disabled=move || main.get() == Probe::Running
                    on:click=move |_| probe::run(main, api::test_stream(request.with_value(|r| r(draft.main_url.get()))))>
                    <Icon icon=I::MonitorPlay class="icon icon--sm" />"Test main stream"
                </button>
                {move || probe::stream_view(main.get())}
            </div>

            <Field label="Substream RTSP URL" optional=true error=Signal::derive(move || errors.get().sub_url)>
                <TextInput value=draft.sub_url placeholder=Signal::derive(move || suggest("stream2")) mono=true />
            </Field>
            <p class="note">
                "A substream is optional. Without one, the main stream is used for live view, motion detection and recording — nothing else needs configuring."
            </p>
            <div class="test-row">
                <button type="button" class="btn btn--secondary btn--sm" disabled=move || sub_empty() || sub.get() == Probe::Running
                    title=move || if sub_empty() { "Enter a substream URL first" } else { "" }
                    on:click=move |_| probe::run(sub, api::test_stream(request.with_value(|r| r(draft.sub_url.get()))))>
                    <Icon icon=I::MonitorPlay class="icon icon--sm" />"Test substream"
                </button>
                {move || probe::stream_view(sub.get())}
            </div>
        </FormSection>
    }
}
