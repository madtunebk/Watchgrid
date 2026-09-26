use leptos::prelude::*;

use crate::api::{LogLevel, RtspTransport, Settings};
use crate::features::settings::save::save;
use crate::ui::form::{Choice, Field, FormSection, NumberInput, RadioCards};
use crate::ui::{follow_server, SaveBar, SaveState};

#[component]
pub fn AdvancedSection(settings: Signal<Settings>) -> impl IntoView {
    let a = settings.get_untracked().advanced;
    let server = crate::api::use_query(crate::api::Topic::Server, None, crate::api::get_server_info);
    let from_env = Signal::derive(move || server.get().and_then(Result::ok).is_some_and(|s| s.log_level_from_env));
    let level = RwSignal::new(a.log_level);
    let transport = RwSignal::new(a.rtsp_transport);
    let reconnect = RwSignal::new(a.reconnect_seconds);
    let state = SaveState::new();

    let dirty = Signal::derive(move || {
        let a = settings.get().advanced;
        (level.get(), transport.get(), reconnect.get()) != (a.log_level, a.rtsp_transport, a.reconnect_seconds)
    });
    let on_save = Callback::new(move |_| {
        let (l, t, r) = (level.get_untracked(), transport.get_untracked(), reconnect.get_untracked());
        save(state, &settings.get_untracked(), move |s| {
            s.advanced.log_level = l;
            s.advanced.rtsp_transport = t;
            s.advanced.reconnect_seconds = r;
        });
    });
    let on_revert = Callback::new(move |_| {
        let a = settings.get_untracked().advanced;
        level.set(a.log_level);
        transport.set(a.rtsp_transport);
        reconnect.set(a.reconnect_seconds);
    });
    follow_server(move || (level.get(), transport.get(), reconnect.get()), move || { let a = settings.get().advanced; (a.log_level, a.rtsp_transport, a.reconnect_seconds) }, on_revert);
    let transports = vec![
        Choice::new(RtspTransport::Tcp, "TCP").tag("Recommended").describe("Reliable over Wi-Fi and VPNs; no lost packets."),
        Choice::new(RtspTransport::Udp, "UDP").describe("Slightly lower latency on clean wired networks. Cameras reconnect when this changes."),
    ];

    view! {
        <div class="settings-tab">
            <FormSection title="Cameras">
                <Field label="RTSP transport"><RadioCards value=transport options=transports name="rtsp-transport" /></Field>
                <Field label="Reconnect delay" hint="Wait before retrying a camera that dropped; doubles with repeated failures, up to a minute (or this delay, if it is longer).">
                    <NumberInput value=reconnect min=1 max=300 suffix="seconds" />
                </Field>
            </FormSection>
            <FormSection title="Diagnostics">
                <Field label="Log level" hint="Applies immediately. Debug is verbose; use it only while troubleshooting. RUST_LOG in watchgrid.env overrides this.">
                    <select class="select" on:change=move |ev| level.set(match event_target_value(&ev).as_str() {
                        "debug" => LogLevel::Debug, "warn" => LogLevel::Warn, "error" => LogLevel::Error, _ => LogLevel::Info })>
                        {[(LogLevel::Debug, "debug", "Debug"), (LogLevel::Info, "info", "Info"), (LogLevel::Warn, "warn", "Warnings"), (LogLevel::Error, "error", "Errors")]
                            .into_iter().map(|(l, v, label)| view! { <option value=v selected=move || level.get() == l>{label}</option> }).collect_view()}
                    </select>
                </Field>
                {move || from_env.get().then(|| view! {
                    <p class="note note--warning">"RUST_LOG is set on the server, so it decides the log level and this setting has no effect. Remove RUST_LOG from watchgrid.env to use it."</p>
                })}
            </FormSection>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
