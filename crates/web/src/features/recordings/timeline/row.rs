use leptos::prelude::*;

use super::scale::Scale;
use crate::api::Recording;
use crate::features::recordings::labels;
use crate::format;

/// One camera's recordings as coloured segments on the day axis.
#[component]
pub fn TrackRow(recordings: Vec<Recording>, scale: Scale, zoom: u8, on_open: Callback<Recording>) -> impl IntoView {
    let empty = recordings.is_empty();
    view! {
        <div class="tl-row">
            {recordings.into_iter().filter_map(|r| {
                let end = r.end_time.unwrap_or_else(chrono::Utc::now);
                let (left, width) = scale.span(r.start_time, end, zoom)?;
                let t = |x: chrono::DateTime<chrono::Utc>| x.with_timezone(&chrono::Local).format("%H:%M:%S").to_string();
                let tip = format!(
                    "{} – {} · {} · {} · {}",
                    t(r.start_time),
                    r.end_time.map(t).unwrap_or_else(|| "now".into()),
                    labels::label(r.reason),
                    format::duration(r.duration),
                    format::bytes(r.file_size)
                );
                let live = r.end_time.is_none();
                Some(view! {
                    <button
                        class=format!("tl-seg tl-seg--{}", labels::css(r.reason))
                        class:tl-seg--live=live
                        class:tl-seg--protected=r.protected
                        style:left=format!("{left:.4}%")
                        style:width=format!("{width:.4}%")
                        title=tip.clone()
                        aria-label=tip
                        on:click=move |_| on_open.run(r.clone())
                    ></button>
                })
            }).collect_view()}
            {empty.then(|| view! { <span class="tl-row__empty">"No recordings"</span> })}
        </div>
    }
}
