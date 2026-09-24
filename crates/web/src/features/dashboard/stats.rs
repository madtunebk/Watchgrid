use leptos::prelude::*;

use crate::api::{ApiResult, Camera, CameraStatus, Event, RecordingReason, StorageStatus, SystemStatus};
use crate::format;
use crate::ui::{Meter, Stat, Tone};

type Res<T> = LocalResource<ApiResult<T>>;

const PENDING: &str = "—";

/// KPI strip. Each tile shows "—" until its data arrives.
#[component]
pub fn StatsRow(cameras: Res<Vec<Camera>>, events: Res<Vec<Event>>, storage: Res<StorageStatus>, system: Res<SystemStatus>) -> impl IntoView {
    let cams = move || cameras.get().and_then(Result::ok);
    let count = move |f: fn(&Camera) -> bool| cams().map(|c| c.iter().filter(|c| f(c)).count());
    let show = |n: Option<usize>| n.map_or(PENDING.to_string(), |n| n.to_string());

    let online = |c: &Camera| c.enabled && c.status == CameraStatus::Online;
    let offline = |c: &Camera| c.enabled && c.status != CameraStatus::Online;
    let recording = |c: &Camera| c.recording_active;

    let offline_names = move || {
        cams().map(|c| c.iter().filter(|c| offline(c)).map(|c| c.name.clone()).collect::<Vec<_>>().join(", ")).unwrap_or_default()
    };
    let recording_detail = move || {
        cams().map(|c| {
            let by = |r: RecordingReason| c.iter().filter(|c| c.recording_active && c.recording_reason == Some(r)).count();
            let (manual, continuous) = (by(RecordingReason::Manual), by(RecordingReason::Continuous));
            let event = c.iter().filter(|c| c.recording_active).count() - manual - continuous;
            let parts: Vec<String> = [(event, "event"), (manual, "manual"), (continuous, "continuous")]
                .into_iter()
                .filter(|(n, _)| *n > 0)
                .map(|(n, label)| format!("{n} {label}"))
                .collect();
            if parts.is_empty() { "Idle".to_string() } else { parts.join(" · ") }
        })
    };
    let today = move || events.get().and_then(Result::ok);
    let storage_pct = move || storage.get().and_then(Result::ok).filter(|s| s.available).map(|s| s.used as f32 / s.total.max(1) as f32 * 100.0);
    let uptime = move || system.get().and_then(Result::ok).map(|s| format!("Up {}", format::uptime(s.uptime)));

    view! {
        <div class="stats">
            <Stat label="Cameras" value=Signal::derive(move || show(cams().map(|c| c.len())))
                detail=Signal::derive(move || count(|c| !c.enabled).map(|n| if n == 0 { "All enabled".into() } else { format!("{n} disabled") }).unwrap_or_default()) />
            <Stat label="Online" tone=Tone::Online value=Signal::derive(move || show(count(online)))
                detail=Signal::derive(move || cams().map(|c| format!("of {}", c.len())).unwrap_or_default()) />
            <Stat label="Offline" value=Signal::derive(move || show(count(offline)))
                tone=Signal::derive(move || if count(offline).unwrap_or(0) > 0 { Tone::Danger } else { Tone::Offline })
                detail=Signal::derive(move || { let n = offline_names(); if n.is_empty() { "None".into() } else { n } }) />
            <Stat label="Recording" tone=Tone::Recording value=Signal::derive(move || show(count(recording)))
                detail=Signal::derive(move || recording_detail().unwrap_or_default()) />
            <Stat label="Events today" tone=Tone::Motion value=Signal::derive(move || show(today().map(|e| e.len())))
                detail=Signal::derive(move || today().and_then(|e| e.first().map(|e| format!("Last at {}", format::time_of_day(e.start_time)))).unwrap_or_else(|| "None yet".into())) />
            <Stat label="Storage" value=Signal::derive(move || storage_pct().map_or(PENDING.into(), |p| format!("{p:.0}%")))
                detail=Signal::derive(move || storage.get().and_then(Result::ok).map(|s| if s.available { format!("{} free", format::bytes(s.free)) } else { "Unavailable".into() }).unwrap_or_default())>
                <Meter value=Signal::derive(move || storage_pct().unwrap_or(0.0)) />
            </Stat>
            <Stat label="Server" tone=Tone::Online value="Running".to_string()
                detail=Signal::derive(move || uptime().unwrap_or_default()) />
        </div>
    }
}
