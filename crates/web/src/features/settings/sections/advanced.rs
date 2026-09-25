use leptos::prelude::*;

use crate::api::{LogLevel, RtspTransport, Settings};
use crate::features::settings::save::save;
use crate::ui::form::{Choice, Field, FormSection, NumberInput, RadioCards};
use crate::ui::{SaveBar, SaveState};

#[component]
pub fn AdvancedSection(settings: Signal<Settings>) -> impl IntoView {
    let a = settings.get_untracked().advanced;
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
        save(state, &settings.get_untracked(), |s| {
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
    let transports = vec![
        Choice::new(RtspTransport::Tcp, "TCP").tag("Recommended").describe("Reliable over Wi-Fi and VPNs; no lost packets."),
        Choice::new(RtspTransport::Udp, "UDP").describe("Slightly lower latency on clean wired networks. Cameras reconnect when this changes."),
    ];

    view! {
        <div class="settings-tab">
            <FormSection title="Cameras">
                <Field label="RTSP transport"><RadioCards value=transport options=transports name="rtsp-transport" /></Field>
                <Field label="Reconnect delay" hint="Wait before retrying a camera that dropped; doubles with repeated failures, up to a minute.">
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
            </FormSection>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
