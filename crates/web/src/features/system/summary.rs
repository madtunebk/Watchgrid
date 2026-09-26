use leptos::prelude::*;

use crate::api::SystemStatus;
use crate::format;
use crate::ui::{I, Icon, Meter};

/// Compact resource overview: CPU, memory, network, streams, uptime.
#[component]
pub fn SystemSummary(status: SystemStatus) -> impl IntoView {
    let mem_pct = status.memory_used as f32 / status.memory_total.max(1) as f32 * 100.0;
    view! {
        <div class="sys-summary">
            <div class="metric">
                <span class="metric__label"><Icon icon=I::Cpu class="icon icon--sm" />"CPU"</span>
                <span class="metric__value">{format!("{:.0}%", status.cpu_usage)}</span>
                <Meter value=status.cpu_usage />
            </div>
            <div class="metric">
                <span class="metric__label"><Icon icon=I::MemoryStick class="icon icon--sm" />"Host memory"</span>
                <span class="metric__value">{format!("{} / {}", format::bytes(status.memory_used), format::bytes(status.memory_total))}</span>
                <Meter value=mem_pct />
            </div>
            <dl class="kv">
                <div><dt><Icon icon=I::ArrowDownUp class="icon icon--sm" />"Network"</dt>
                    <dd>{format!("↓ {}  ↑ {}", format::rate(status.network_rx), format::rate(status.network_tx))}</dd></div>
                <div><dt><Icon icon=I::MonitorPlay class="icon icon--sm" />"RTSP streams"</dt>
                    <dd>{status.active_streams}</dd></div>
                <div><dt><Icon icon=I::Clock class="icon icon--sm" />"Uptime"</dt>
                    <dd>{format::uptime(status.uptime)}</dd></div>
            </dl>
        </div>
    }
}
