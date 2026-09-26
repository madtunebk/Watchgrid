use leptos::prelude::*;

use crate::api::{ApiResult, Camera, CameraStatus, EventPage, RecordingReason, StorageStatus, SystemStatus};
use crate::format;
use crate::ui::{Meter, Stat, Tone};

type Res<T> = LocalResource<ApiResult<T>>;

const PENDING: &str = "—";

/// KPI strip. Each tile shows "—" until its data arrives.
#[component]
pub fn StatsRow(cameras: Res<Vec<Camera>>, events: Res<EventPage>, storage: Res<StorageStatus>, system: Res<SystemStatus>) -> impl IntoView {
    let cams = move || cameras.get().and_then(Result::ok);
    let count = move |f: fn(&Camera) -> bool| cams().map(|c| c.iter().filter(|c| f(c)).count());
    let show = |n: Option<usize>| n.map_or(PENDING.to_string(), |n| n.to_string());

    let online = |c: &Camera| c.enabled && c.status == CameraStatus::Online;
    // Connecting (start-up, reconnect) is not "offline"; it is shown apart.
    let offline = |c: &Camera| c.enabled && matches!(c.status, CameraStatus::Offline | CameraStatus::Error);
    let connecting = |c: &Camera| c.enabled && c.status == CameraStatus::Connecting;
    let recording = |c: &Camera| c.recording_active;

    let offline_names = move || {
        cams().map(|c| c.iter().filter(|c| offline(c)).map(|c| c.name.clone()).collect::<Vec<_>>().join(", ")).unwrap_or_default()
    };
    let recording_detail = move || {
        cams().map(|c| {
            let by = |rs: &[RecordingReason]| c.iter().filter(|c| c.recording_active && c.recording_reason.is_some_and(|r| rs.contains(&r))).count();
            let parts: Vec<String> = [
                (by(&[RecordingReason::Motion, RecordingReason::Event]), "event"),
                (by(&[RecordingReason::Manual]), "manual"),
                (by(&[RecordingReason::Continuous]), "continuous"),
                (by(&[RecordingReason::Scheduled]), "scheduled"),
                (by(&[RecordingReason::Api]), "API"),
            ]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, label)| format!("{n} {label}"))
            .collect();
            if parts.is_empty() { "Idle".to_string() } else { parts.join(" · ") }
        })
    };
    let today = move || events.get().and_then(Result::ok);
    let storage_pct = move || storage.get().and_then(Result::ok).filter(|s| s.available).map(|s| s.used as f32 / s.total.max(1) as f32 * 100.0);
    // The server as this page last heard from it — never an invented "Running".
    let server = move || match system.get() {
        None => (PENDING.to_string(), Tone::Offline, String::new()),
        Some(Ok(s)) => ("Running".to_string(), Tone::Online, format!("Up {}", format::uptime(s.uptime))),
        Some(Err(e)) => ("Not responding".to_string(), Tone::Danger, e.to_string()),
    };

    view! {
        <div class="stats">
            <Stat label="Cameras" href="/cameras" value=Signal::derive(move || show(cams().map(|c| c.len())))
                detail=Signal::derive(move || count(|c| !c.enabled).map(|n| if n == 0 { "All enabled".into() } else { format!("{n} disabled") }).unwrap_or_default()) />
            <Stat label="Online" href="/cameras?status=online" tone=Tone::Online value=Signal::derive(move || show(count(online)))
                detail=Signal::derive(move || cams().map(|c| format!("of {}", c.len())).unwrap_or_default()) />
            <Stat label="Offline" href="/cameras?status=offline" value=Signal::derive(move || show(count(offline)))
                tone=Signal::derive(move || if count(offline).unwrap_or(0) > 0 { Tone::Danger } else { Tone::Offline })
                detail=Signal::derive(move || {
                    let names = offline_names();
                    let waiting = count(connecting).unwrap_or(0);
                    match (names.is_empty(), waiting) {
                        (true, 0) => "None".to_string(),
                        (true, n) => format!("{n} connecting"),
                        (false, 0) => names,
                        (false, n) => format!("{names} (+{n} connecting)"),
                    }
                }) />
            <Stat label="Recording" href="/cameras?status=recording" tone=Signal::derive(move || if count(recording).unwrap_or(0) > 0 { Tone::Recording } else { Tone::Offline }) value=Signal::derive(move || show(count(recording)))
                detail=Signal::derive(move || recording_detail().unwrap_or_default()) />
            <Stat label="Events today" href="/events" tone=Tone::Motion value=Signal::derive(move || show(today().map(|p| p.total as usize)))
                detail=Signal::derive(move || today().and_then(|p| p.events.first().map(|e| format!("Last at {}", format::time_of_day(e.start_time)))).unwrap_or_else(|| "None yet".into())) />
            <Stat label="Storage" href="/storage" value=Signal::derive(move || storage_pct().map_or(PENDING.into(), |p| format!("{p:.0}%")))
                detail=Signal::derive(move || storage.get().and_then(Result::ok).map(|s| if s.available { format!("{} free", format::bytes(s.free)) } else { "Unavailable".into() }).unwrap_or_default())>
                <Meter value=Signal::derive(move || storage_pct().unwrap_or(0.0)) />
            </Stat>
            <Stat label="Server" href="/system" tone=Signal::derive(move || server().1) value=Signal::derive(move || server().0)
                detail=Signal::derive(move || server().2) />
        </div>
    }
}
