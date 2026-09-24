use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{Event, Recording};
use crate::features::events::widgets::EventChip;
use crate::format;
use crate::ui::{Meter, Tone};

fn when(t: chrono::DateTime<chrono::Utc>) -> String {
    t.with_timezone(&chrono::Local).format("%a %-d %b %Y, %H:%M:%S").to_string()
}

#[component]
pub fn EventFacts(event: Event, recording: Option<Recording>, camera_name: String) -> impl IntoView {
    let ended = match event.end_time {
        Some(t) => when(t),
        None => "In progress".into(),
    };
    view! {
        <dl class="facts">
            <div><dt>"Camera"</dt><dd><A href=format!("/cameras/{}", event.camera_id) attr:class="link">{camera_name}</A></dd></div>
            <div><dt>"Type"</dt><dd><EventChip kind=event.kind /></dd></div>
            <div><dt>"Started"</dt><dd>{when(event.start_time)}</dd></div>
            <div><dt>"Ended"</dt><dd>{ended}</dd></div>
            <div><dt>"Duration"</dt><dd class="mono">{format::duration(event.duration)}</dd></div>
            <div><dt>"Source"</dt><dd>{event.source.clone()}</dd></div>
            {recording.map(|r| view! {
                <div><dt>"Clip"</dt><dd class="mono">{format!("{} · {}", format::duration(r.duration), format::bytes(r.file_size))}</dd></div>
            })}
        </dl>
        <div class="detections">
            <h3 class="detections__title">"Detection"</h3>
            {if event.detections.is_empty() {
                view! { <p class="muted">"No object classification for this event."</p> }.into_any()
            } else {
                event.detections.into_iter().map(|d| {
                    let pct = d.confidence * 100.0;
                    let tone = if pct >= 80.0 { Tone::Online } else { Tone::Warning };
                    view! {
                        <div class="detection">
                            <span class="detection__label">{d.label}</span>
                            <Meter value=pct tone />
                            <span class="detection__pct">{format!("{pct:.0}%")}</span>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </div>
    }
}
