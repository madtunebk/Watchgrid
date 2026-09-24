use leptos::prelude::*;

use crate::api::Settings;
use crate::features::settings::save::save;
use crate::ui::form::{Field, FormSection, NumberInput, Switch, TextInput};
use crate::ui::{SaveBar, SaveState};

#[component]
pub fn NetworkSection(settings: Signal<Settings>) -> impl IntoView {
    let n = settings.get_untracked().network;
    let bind = RwSignal::new(n.http_bind);
    let port = RwSignal::new(n.http_port as u32);
    let https = RwSignal::new(n.https_enabled);
    let https_port = RwSignal::new(n.https_port as u32);
    let state = SaveState::new();

    let dirty = Signal::derive(move || {
        let n = settings.get().network;
        (bind.get(), port.get(), https.get(), https_port.get()) != (n.http_bind, n.http_port as u32, n.https_enabled, n.https_port as u32)
    });
    let moves = Signal::derive(move || {
        let n = settings.get().network;
        bind.get() != n.http_bind || port.get() != n.http_port as u32
    });
    let on_save = Callback::new(move |_| {
        let (b, p, h, hp) = (bind.get_untracked(), port.get_untracked(), https.get_untracked(), https_port.get_untracked());
        save(state, &settings.get_untracked(), |s| {
            s.network.http_bind = b.trim().into();
            s.network.http_port = p as u16;
            s.network.https_enabled = h;
            s.network.https_port = hp as u16;
        });
    });
    let on_revert = Callback::new(move |_| {
        let n = settings.get_untracked().network;
        bind.set(n.http_bind);
        port.set(n.http_port as u32);
        https.set(n.https_enabled);
        https_port.set(n.https_port as u32);
    });

    view! {
        <div class="settings-tab">
            <FormSection title="Web interface" description="Where Watchgrid listens for browsers and API clients.">
                <div class="form-grid">
                    <Field label="Bind address" hint="0.0.0.0 = all interfaces, 127.0.0.1 = this machine only">
                        <TextInput value=bind placeholder="0.0.0.0" mono=true />
                    </Field>
                    <Field label="HTTP port"><NumberInput value=port min=1 max=65535 /></Field>
                </div>
                <Show when=move || moves.get()>
                    <p class="note note--warn">"Saving moves the web interface to the new address or port. Only the web listener restarts; cameras keep recording. Open the new address afterwards."</p>
                </Show>
            </FormSection>
            <FormSection title="HTTPS" description="Encrypted access with a certificate.">
                <Switch checked=https label="Enable HTTPS" description="Arrives with the web backend; the setting is stored now." />
                <Field label="HTTPS port"><NumberInput value=https_port min=1 max=65535 disabled=Signal::derive(move || !https.get()) /></Field>
            </FormSection>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
