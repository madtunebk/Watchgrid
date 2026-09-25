use std::time::Duration;

use leptos::prelude::*;

use super::capacity::CapacitySummary;
use super::logs::LogViewer;
use super::metrics::Metrics;
use crate::api::{self, ServerHealth, Topic, use_query};
use crate::format;
use crate::ui::{Page, Panel, Skeleton, async_view};

#[component]
pub fn SystemPage() -> impl IntoView {
    let status = use_query(Topic::System, Some(Duration::from_secs(2)), api::get_system_status);
    let server = use_query(Topic::Server, None, api::get_server_info);
    let capacity = use_query(Topic::Cameras, None, api::get_capacity);

    let subtitle = Signal::derive(move || {
        let s = server.get().and_then(Result::ok);
        let up = status.get().and_then(Result::ok).map(|st| format::uptime(st.uptime));
        match (s, up) {
            (Some(s), Some(up)) => format!("{} · v{} · up {up}", s.name, s.version),
            (Some(s), None) => format!("{} · v{}", s.name, s.version),
            _ => String::new(),
        }
    });

    view! {
        <Page title="System" subtitle>
            <Metrics status />
            <div class="system-layout">
                <Panel title="Capacity">
                    {async_view(capacity, || view! { <Skeleton lines=5 /> }.into_any(), |estimate| view! { <CapacitySummary estimate /> })}
                </Panel>
                <Panel title="Server">
                    {async_view(server, || view! { <Skeleton lines=4 /> }.into_any(), move |s| {
                        let health = match s.health {
                            ServerHealth::Running => "Running",
                            ServerHealth::Degraded => "Degraded",
                            ServerHealth::Stopped => "Stopped",
                        };
                        let started = s.started_at.with_timezone(&chrono::Local);
                        let started = format!("{}, {}", started.format("%a %-d %b %Y"), crate::format::time_hm(started));
                        let hw = capacity.get().and_then(Result::ok).map(|c| c.hardware);
                        view! {
                            <dl class="facts">
                                <div><dt>"Name"</dt><dd>{s.name}</dd></div>
                                <div><dt>"Version"</dt><dd class="mono">{s.version}</dd></div>
                                <div><dt>"Status"</dt><dd class="text-online">{health}</dd></div>
                                <div><dt>"Started"</dt><dd>{started}</dd></div>
                                {hw.map(|h| view! {
                                    <div><dt>"CPU"</dt><dd>{format!("{} ({} cores)", h.cpu_model, h.cpu_cores)}</dd></div>
                                    <div><dt>"Decoding"</dt><dd>"Not needed: video is stored and streamed as the camera sends it"</dd></div>
                                    <div><dt>"Memory"</dt><dd>{format::bytes(h.memory_total)}</dd></div>
                                })}
                            </dl>
                        }
                    })}
                </Panel>
            </div>
            <Panel title="Recent log">
                <LogViewer />
            </Panel>
        </Page>
    }
}
