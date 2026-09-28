//! Day timeline: one row per camera, recordings as segments and events as
//! dots, zoomable. A click on a row plays that camera from that moment, a
//! click on a dot from just before that event; the playing position is
//! drawn as a line.

mod row;
mod scale;

use std::time::Duration;

use leptos::html::Div;
use leptos::prelude::*;

use crate::api::{Camera, Event, Recording};
use crate::clock::use_now;
use crate::features::recordings::labels::LEGEND;
use row::TrackRow;
pub use scale::Scale;

#[component]
pub fn Timeline(
    cameras: Vec<Camera>,
    recordings: Vec<Recording>,
    /// The day's detections, all cameras.
    events: Vec<Event>,
    scale: Scale,
    #[prop(into)] zoom: Signal<u8>,
    on_seek: Callback<(String, chrono::DateTime<chrono::Utc>)>,
    /// Where playback is, if something plays.
    #[prop(into)] playhead: Signal<Option<chrono::DateTime<chrono::Utc>>>,
) -> impl IntoView {
    let scroller = NodeRef::<Div>::new();
    let now = use_now(Duration::from_secs(30));

    // The time at the centre of the view, as a fraction of the day. It is
    // kept on every scroll and restored after zooming, so zoom keeps what you
    // were looking at. Today starts centred on "now".
    let now_utc = chrono::Utc::now();
    let centre = StoredValue::new(if scale.contains(now_utc) { scale.pct(now_utc) / 100.0 } else { 0.5 });
    let on_scroll = move |_| {
        if let Some(el) = scroller.get_untracked() {
            let width = (el.scroll_width() as f64).max(1.0);
            centre.set_value((el.scroll_left() as f64 + el.client_width() as f64 / 2.0) / width);
        }
    };
    Effect::new(move || {
        zoom.track();
        let Some(el) = scroller.get() else { return };
        let c = centre.get_value();
        request_animation_frame(move || {
            let target = c * el.scroll_width() as f64 - el.client_width() as f64 / 2.0;
            el.set_scroll_left(target.max(0.0) as i32);
        });
    });

    let rows: Vec<(Camera, Vec<Recording>, Vec<Event>)> = cameras
        .into_iter()
        .map(|c| {
            let mine = recordings.iter().filter(|r| r.camera_id == c.id).cloned().collect();
            let seen = events.iter().filter(|e| e.camera_id == c.id).cloned().collect();
            (c, mine, seen)
        })
        .collect();
    let names: Vec<(String, String)> = rows.iter().map(|(c, _, _)| (c.id.clone(), c.name.clone())).collect();

    view! {
        <div class="tl">
            <div class="tl__names">
                <div class="tl__corner"></div>
                {names.into_iter().map(|(id, name)| {
                    let title = name.clone();
                    view! { <a class="tl__name truncate" href=format!("/cameras/{id}") title=title>{name}</a> }
                }).collect_view()}
            </div>
            <div class="tl__scroll" node_ref=scroller on:scroll=on_scroll>
                <div class="tl__canvas" style:width=move || format!("{}%", zoom.get() as u32 * 100)>
                    <div class="tl-axis">
                        {move || scale.ticks(zoom.get()).into_iter().map(|(pct, label)| view! {
                            <span class="tl-axis__tick" style:left=format!("{pct:.4}%")>{label}</span>
                        }).collect_view()}
                    </div>
                    <div class="tl__rows">
                        {move || {
                            let z = zoom.get();
                            rows.clone().into_iter().map(|(c, recs, evs)| view! { <TrackRow camera_id=c.id recordings=recs events=evs scale zoom=z on_seek /> }).collect_view()
                        }}
                        <div class="tl__grid" aria-hidden="true">
                            {move || scale.ticks(zoom.get()).into_iter().map(|(pct, _)| view! {
                                <span style:left=format!("{pct:.4}%")></span>
                            }).collect_view()}
                        </div>
                        {move || {
                            let t = now.get().to_utc();
                            scale.contains(t).then(|| view! { <div class="tl__now" style:left=format!("{:.4}%", scale.pct(t)) title="Now"></div> })
                        }}
                        {move || playhead.get().filter(|t| scale.contains(*t)).map(|t| view! {
                            <div class="tl__playhead" style:left=format!("{:.4}%", scale.pct(t)) title="Playing"></div>
                        })}
                    </div>
                </div>
            </div>
        </div>
        <div class="tl-legend">
            {LEGEND.iter().map(|(css, label)| view! { <span class=format!("tl-legend__item tl-legend__item--{css}")>{*label}</span> }).collect_view()}
            <span class="tl-legend__item tl-legend__item--event">"Event (dot)"</span>
            <span class="tl-legend__hint">"Click a row to play from that moment, a dot to see that event · + / − to zoom"</span>
        </div>
    }
}
