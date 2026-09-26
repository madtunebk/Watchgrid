//! Live resource tiles with a short client-side history for sparklines.

use leptos::prelude::*;

use crate::api::{ApiResult, SystemStatus};
use crate::format;
use crate::ui::{Meter, Sparkline, Stat, Tone};

/// Samples kept per series (≈ 2 minutes at the 2 s poll).
const HISTORY: usize = 60;

#[derive(Clone, Default, PartialEq)]
struct History {
    cpu: Vec<f32>,
    memory: Vec<f32>,
    rx: Vec<f32>,
    tx: Vec<f32>,
}

impl History {
    fn push(&mut self, s: &SystemStatus) {
        let push = |v: &mut Vec<f32>, x: f32| {
            v.push(x);
            if v.len() > HISTORY {
                v.remove(0);
            }
        };
        push(&mut self.cpu, s.cpu_usage);
        push(&mut self.memory, s.memory_used as f32 / s.memory_total.max(1) as f32 * 100.0);
        push(&mut self.rx, s.network_rx as f32 * 8.0 / 1e6);
        push(&mut self.tx, s.network_tx as f32 * 8.0 / 1e6);
    }
}

#[component]
pub fn Metrics(status: LocalResource<ApiResult<SystemStatus>>) -> impl IntoView {
    let history = RwSignal::new(History::default());
    let current = Memo::new(move |_| status.get().and_then(Result::ok));
    Effect::new(move || {
        if let Some(s) = current.get() {
            history.update(|h| h.push(&s));
        }
    });
    let v = move |f: fn(&SystemStatus) -> String| Signal::derive(move || current.get().map_or("—".to_string(), |s| f(&s)));
    let series = move |f: fn(&History) -> Vec<f32>| Signal::derive(move || f(&history.get()));
    let net_max = Signal::derive(move || {
        let h = history.get();
        h.rx.iter().chain(&h.tx).cloned().fold(1.0f32, f32::max) * 1.2
    });
    let mem_pct = Signal::derive(move || current.get().map_or(0.0, |s| s.memory_used as f32 / s.memory_total.max(1) as f32 * 100.0));
    let disk = Signal::derive(move || current.get().map_or(0.0, |s| s.disk_usage));

    view! {
        <div class="stats metrics">
            <Stat label="CPU" value=v(|s| format!("{:.0}%", s.cpu_usage))>
                <Sparkline values=series(|h| h.cpu.clone()) />
            </Stat>
            <Stat label="Memory" value=v(|s| format::bytes(s.process_memory))
                detail=Signal::derive(move || current.get().map_or(String::new(), |s| format!("Watchgrid · host {} of {} ({:.0}%)", format::bytes(s.memory_used), format::bytes(s.memory_total), mem_pct.get()))) >
                <Sparkline values=series(|h| h.memory.clone()) tone="stream" />
            </Stat>
            <Stat label="Disk" value=v(|s| format!("{:.0}%", s.disk_usage)) detail="Recording volume".to_string()>
                <Meter value=disk />
            </Stat>
            <Stat label="Network in" value=v(|s| format::rate(s.network_rx)) detail="From cameras".to_string()>
                {move || view! { <Sparkline values=series(|h| h.rx.clone()) max=net_max.get() tone="online" /> }}
            </Stat>
            <Stat label="Network out" value=v(|s| format::rate(s.network_tx)) detail="To viewers".to_string()>
                {move || view! { <Sparkline values=series(|h| h.tx.clone()) max=net_max.get() tone="motion" /> }}
            </Stat>
            <Stat label="RTSP connections" value=v(|s| s.active_streams.to_string()) tone=Tone::Stream />
            <Stat label="Recording" value=v(|s| s.active_recordings.to_string())
                tone=Signal::derive(move || if current.get().is_some_and(|s| s.active_recordings > 0) { Tone::Recording } else { Tone::Offline }) />
            <Stat label="Cameras connected" value=v(|s| format!("{} / {}", s.connected_cameras, s.total_cameras)) tone=Tone::Online>
                <Meter value=Signal::derive(move || current.get().map_or(0.0, |s| s.connected_cameras as f32 / s.total_cameras.max(1) as f32 * 100.0)) tone=Tone::Online />
            </Stat>
            <Stat label="Uptime" value=v(|s| format::uptime(s.uptime)) detail="Since last start".to_string() />
        </div>
    }
}
