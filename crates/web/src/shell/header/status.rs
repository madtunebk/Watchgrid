//! Live status indicators: server health, active recordings, API link, clock.

use std::time::Duration;

use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{self, CheckLevel, ConnectionState, ServerHealth, Topic, use_connection, use_query};
use crate::clock::use_now;
use crate::format;
use crate::ui::{Dot, I, Icon, Tone};

/// The NVR's health from its checks (same poll as the recording count);
/// the tooltip names what needs attention.
#[component]
pub fn ServerStatus() -> impl IntoView {
    let status = use_query(Topic::System, Some(Duration::from_secs(5)), api::get_system_status);
    move || {
        let (tone, label, title) = match status.get()? {
            Ok(s) => {
                let (tone, label) = match s.health {
                    ServerHealth::Running => (Tone::Online, "NVR running"),
                    ServerHealth::Degraded => (Tone::Warning, "NVR: needs attention"),
                    ServerHealth::Stopped => (Tone::Danger, "NVR problem"),
                };
                let problems: Vec<String> = s.checks.iter().filter(|c| c.level != CheckLevel::Ok).map(|c| format!("{}: {}", c.name, c.detail)).collect();
                (tone, label, if problems.is_empty() { "Every check passes".to_string() } else { problems.join("\n") })
            }
            Err(e) => (Tone::Offline, "NVR not responding", e.to_string()),
        };
        Some(view! {
            <A href="/system" attr:class="server-status" attr:title=title>
                <Dot tone />
                {label}
            </A>
        })
    }
}

#[component]
pub fn RecordingCount() -> impl IntoView {
    let status = use_query(Topic::System, Some(Duration::from_secs(5)), api::get_system_status);
    move || {
        let n = status.get().and_then(Result::ok).map(|s| s.active_recordings).unwrap_or(0);
        (n > 0).then(|| {
            let title = format!("{n} camera{} recording", if n == 1 { "" } else { "s" });
            view! {
                <A href="/cameras" attr:class="rec-pill" attr:title=title>
                    <Dot tone=Tone::Recording pulse=true />
                    {format!("REC {n}")}
                </A>
            }
        })
    }
}

#[component]
pub fn ConnectionIndicator() -> impl IntoView {
    let state = use_connection();
    let label = move || match state.get() {
        ConnectionState::Connected => "Connected",
        ConnectionState::Connecting => "Reconnecting…",
        ConnectionState::Disconnected => "Disconnected",
    };
    view! {
        <span
            role="status"
            class="conn"
            class:conn--connected=move || state.get() == ConnectionState::Connected
            class:conn--connecting=move || state.get() == ConnectionState::Connecting
            class:conn--disconnected=move || state.get() == ConnectionState::Disconnected
            title=move || format!("Link to NVR: {}", label())
        >
            {move || {
                let icon = if state.get() == ConnectionState::Disconnected { I::WifiOff } else { I::Wifi };
                view! { <Icon icon /> }
            }}
            <span class:sr-only=move || state.get() == ConnectionState::Connected>{label}</span>
        </span>
    }
}

#[component]
pub fn Clock() -> impl IntoView {
    let now = use_now(Duration::from_secs(1));
    view! { <span class="clock">{move || format::clock(now.get())}</span> }
}
