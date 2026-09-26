use leptos::prelude::*;

use super::scale::Scale;
use crate::api::Recording;
use crate::features::recordings::labels;
use crate::format;

/// Where along `row` a click landed, 0 to 1.
fn fraction(row: &web_sys::Element, ev: &leptos::ev::MouseEvent) -> f64 {
    let rect = row.get_bounding_client_rect();
    (f64::from(ev.client_x()) - rect.left()) / rect.width().max(1.0)
}

fn element(target: Option<web_sys::EventTarget>) -> Option<web_sys::Element> {
    target.and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok())
}

/// One camera's recordings as coloured segments on the day axis. A click
/// anywhere on the row plays from that moment (`on_seek`); a click on a
/// clip plays that clip, even where it is drawn wider than it lasts.
#[component]
pub fn TrackRow(camera_id: String, recordings: Vec<Recording>, scale: Scale, zoom: u8, on_seek: Callback<(String, chrono::DateTime<chrono::Utc>)>) -> impl IntoView {
    let empty = recordings.is_empty();
    let seek = {
        let camera_id = camera_id.clone();
        move |ev: leptos::ev::MouseEvent| {
            let Some(row) = element(ev.current_target()) else { return };
            on_seek.run((camera_id.clone(), scale.at(fraction(&row, &ev))));
        }
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
                // Short clips have a minimum width: the time under the pointer
                // may lie past their end, so keep it inside the clip.
                let (start, camera_id) = (r.start_time, camera_id.clone());
                let play = move |ev: leptos::ev::MouseEvent| {
                    ev.stop_propagation();
                    let Some(row) = element(ev.current_target()).and_then(|b| b.parent_element()) else { return };
                    let last = end - chrono::Duration::seconds(1);
                    on_seek.run((camera_id.clone(), scale.at(fraction(&row, &ev)).clamp(start, last.max(start))));
                };
                Some(view! {
                    <button
                        on:click=play
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
