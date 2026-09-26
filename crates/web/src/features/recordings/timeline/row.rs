use leptos::prelude::*;

use super::scale::Scale;
use crate::api::Recording;
use crate::features::recordings::labels;
use crate::format;

/// One camera's recordings as coloured segments on the day axis. A click
/// anywhere on the row plays from that moment (`on_seek`).
#[component]
pub fn TrackRow(camera_id: String, recordings: Vec<Recording>, scale: Scale, zoom: u8, on_seek: Callback<(String, chrono::DateTime<chrono::Utc>)>) -> impl IntoView {
    let empty = recordings.is_empty();
    let seek = move |ev: leptos::ev::MouseEvent| {
        let Some(row) = ev.current_target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()) else { return };
        let rect = row.get_bounding_client_rect();
        let fraction = (f64::from(ev.client_x()) - rect.left()) / rect.width().max(1.0);
        on_seek.run((camera_id.clone(), scale.at(fraction)));
    };
    view! {
        <div class="tl-row" on:click=seek title="Click to play from here">
            {recordings.into_iter().filter_map(|r| {
                let end = r.end_time.unwrap_or_else(chrono::Utc::now);
                let (left, width) = scale.span(r.start_time, end, zoom)?;
                let t = |x: chrono::DateTime<chrono::Utc>| crate::format::time_hms(x.with_timezone(&chrono::Local));
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
                        class:tl-seg--protected=r.is_protected()
                        style:left=format!("{left:.4}%")
                        style:width=format!("{width:.4}%")
                        title=tip.clone()
                        aria-label=tip
                    ></button>
                })
            }).collect_view()}
            {empty.then(|| view! { <span class="tl-row__empty">"No recordings"</span> })}
        </div>
    }
}
