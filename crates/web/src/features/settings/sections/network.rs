use leptos::prelude::*;

use crate::api::Settings;
use crate::ui::form::FormSection;

/// Read-only: the listen address is set where the service is installed
/// (`watchgrid.env`), and HTTPS comes from a reverse proxy in front.
#[component]
pub fn NetworkSection(settings: Signal<Settings>) -> impl IntoView {
    let listen = move || {
        let n = settings.get().network;
        format!("{}:{}", n.http_bind, n.http_port)
    };
    let https = web_sys::window().and_then(|w| w.location().protocol().ok()).is_some_and(|p| p == "https:");

    view! {
        <div class="settings-tab">
            <FormSection title="Web interface" description="Where Watchgrid listens for browsers and API clients.">
                <dl class="facts">
                    <div><dt>"Listening on"</dt><dd class="mono">{listen}</dd></div>
                </dl>
                <p class="note">
                    "Set with " <code>"WATCHGRID_BIND"</code> " in " <code>"watchgrid.env"</code>
                    " (Docker) or " <code>"/etc/watchgrid/watchgrid.env"</code> ", then restart the service. "
                    <code>"0.0.0.0:8090"</code> " = all interfaces, " <code>"127.0.0.1:8090"</code> " = this machine only (behind a reverse proxy)."
                </p>
            </FormSection>
            <FormSection title="HTTPS" description="Encrypted access with a certificate.">
                <dl class="facts">
                    <div><dt>"This page"</dt><dd>{if https { "Served over HTTPS" } else { "Plain HTTP" }}</dd></div>
                </dl>
                <p class="note">
                    "Watchgrid serves HTTP; put a reverse proxy with a certificate in front (nginx, or the NAS's own, e.g. Synology DSM). "
                    "Then set " <code>"WATCHGRID_SECURE_COOKIES=1"</code> " and " <code>"WATCHGRID_TRUSTED_PROXIES"</code>
                    " to the proxy's address. Examples: " <code>"deploy/nginx/"</code> "."
                </p>
            </FormSection>
        </div>
    }
}
