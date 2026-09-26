//! Clip player shared by events and recordings. Clips with a real file
//! (`Clip::src`) play in a `<video>` element driven by these controls;
//! mock clips keep a simulated surface with the same timeline.

use std::time::Duration;

use leptos::html::{Div, Video};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use super::clip::Clip;
use crate::clock::use_interval;
use crate::features::events::EventChip;
use crate::format;
use crate::ui::{I, Icon, fullscreen};

const TICK: f32 = 0.25;
const SPEEDS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];

#[component]
pub fn Player(
    clip: Clip,
    /// Large ("theater") layout toggle, when the page offers one.
    #[prop(optional)]
    theater: Option<RwSignal<bool>>,
) -> impl IntoView {
    let clip_start = clip.start;
    // A clip still recording grows: its length comes from each fresh copy.
    let total = RwSignal::new(clip.duration);
    let growing = clip.growing;
    // Without a highlight the whole clip is "the event" for the timeline colours.
    let (pre, ev_len) = clip.highlight.unwrap_or((0.0, 0.0));
    let post = move || (total.get() - pre - ev_len).max(0.0);
    let pct = move |v: f32| format!("{:.2}%", v / total.get().max(0.001) * 100.0);

    let position = RwSignal::new(pre); // start at the moment of the event (or 0)
    let playing = RwSignal::new(false);
    let speed = RwSignal::new(1.0f32);
    // Recordings play with sound (a click started them); this silences it.
    let muted = RwSignal::new(false);
    let src = clip.src.clone();
    let real = src.is_some();
    let video = NodeRef::<Video>::new();
    // Simulated playback for clips without a file.
    use_interval(Duration::from_millis((TICK * 1000.0) as u64), move || {
        if !real && playing.get_untracked() {
            let next = position.get_untracked() + TICK * speed.get_untracked();
            if next >= total.get_untracked() {
                position.set(total.get_untracked());
                playing.set(false);
            } else {
                position.set(next);
            }
        }
    });
    let seek = move |t: f32| {
        let t = t.clamp(0.0, total.get_untracked());
        position.set(t);
        if let Some(v) = video.get_untracked() {
            v.set_current_time(f64::from(t));
        }
    };
    // Real playback: the element follows the controls, the timeline follows the element.
    Effect::new(move |_| {
        let play = playing.get();
        let Some(v) = video.get() else { return };
        if play {
            if let Ok(p) = v.play() {
                spawn_local(async move {
                    if JsFuture::from(p).await.is_err() {
                        playing.set(false);
                    }
                });
            }
        } else {
            let _ = v.pause();
        }
    });
    Effect::new(move |_| {
        let rate = f64::from(speed.get());
        if let Some(v) = video.get() {
            v.set_playback_rate(rate);
        }
    });
    let on_time = move |_| {
        if let Some(v) = video.get_untracked() {
            position.set(v.current_time() as f32);
        }
    };
    // Where to continue after fetching a longer copy of a growing clip.
    let resume_at = RwSignal::new(None::<f64>);
    let on_loaded = move |_| {
        let Some(v) = video.get_untracked() else { return };
        if growing && v.duration().is_finite() {
            total.set(v.duration() as f32);
        }
        match resume_at.get_untracked() {
            Some(t) => {
                resume_at.set(None);
                v.set_current_time(t);
                playing.set(true);
                let _ = v.play();
            }
            None if pre > 0.0 => v.set_current_time(f64::from(pre)),
            None => {}
        }
    };
    // At the end of a clip that is still recording: load what was added.
    let on_ended = {
        let src = src.clone();
        move |_| {
            let (Some(v), Some(src)) = (video.get_untracked(), src.as_ref()) else { return };
            if !growing {
                playing.set(false);
                return;
            }
            resume_at.set(Some(v.current_time()));
            v.set_src(&format!("{src}?at={}", js_sys::Date::now() as u64));
        }
    };
    let stage = NodeRef::<Div>::new();

    let on_bar_click = move |ev: leptos::ev::MouseEvent| {
        if let Some(el) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let rect = el.get_bounding_client_rect();
            let frac = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0) as f32;
            seek(frac * total.get_untracked());
        }
    };
    let clock = move || crate::format::date_time((clip_start + chrono::Duration::milliseconds((position.get() * 1000.0) as i64)).with_timezone(&chrono::Local));
    let in_event = move || { let p = position.get(); p >= pre && p <= pre + ev_len };
    let camera_name = clip.camera_name.clone();
    let has_box = clip.detection_box;

    view! {
        <div class="player">
            <div class="player__stage" node_ref=stage on:click=move |_| playing.update(|p| *p = !*p)>
                {match src {
                    Some(src) => view! {
                        <video class="player__video" node_ref=video src=src preload="metadata" playsinline=true prop:muted=muted
                            on:timeupdate=on_time on:loadedmetadata=on_loaded on:ended=on_ended></video>
                    }.into_any(),
                    None => view! {
                        <div class="player__surface" class:player__surface--playing=playing>
                            <Icon icon=I::Film class="icon icon--xl" />
                        </div>
                    }.into_any(),
                }}
                {clip.kind.map(|kind| view! { <div class="player__tags"><EventChip kind /></div> })}
                {has_box.then(|| view! { <div class="player__box" class:player__box--visible=in_event></div> })}
                <div class="player__osd"><span>{camera_name}</span><span>{clock}</span></div>
                {growing.then(|| view! { <span class="player__growing" title="The clip grows while it records"><span class="ptag__dot pulse"></span>"Recording"</span> })}
                <Show when=move || !playing.get()>
                    <span class="player__big-play" aria-hidden="true"><Icon icon=I::Play class="icon icon--xl" /></span>
                </Show>
            </div>

            <div class="player__timeline" on:click=on_bar_click title="Click to seek">
                {if clip.highlight.is_some() {
                    view! {
                        <div class="player__seg player__seg--pre" style:width=move || pct(pre)></div>
                        <div class="player__seg player__seg--event" style:width=move || pct(ev_len)></div>
                        <div class="player__seg player__seg--post" style:width=move || pct(post())></div>
                    }.into_any()
                } else {
                    view! { <div class="player__seg player__seg--plain" style:width="100%"></div> }.into_any()
                }}
                <div class="player__head" style:left=move || pct(position.get())></div>
            </div>

            <div class="player__controls">
                <button class="icon-btn" aria-label=move || if playing.get() { "Pause" } else { "Play" }
                    on:click=move |_| playing.update(|p| *p = !*p)>
                    {move || view! { <Icon icon=if playing.get() { I::Pause } else { I::Play } /> }}
                </button>
                <button class="icon-btn" aria-label="Back 10 seconds" title="Back 10 s" on:click=move |_| seek(position.get_untracked() - 10.0)>
                    <Icon icon=I::RotateCcw />
                </button>
                <button class="icon-btn" aria-label="Forward 10 seconds" title="Forward 10 s" on:click=move |_| seek(position.get_untracked() + 10.0)>
                    <Icon icon=I::RotateCw />
                </button>
                {clip.highlight.is_some().then(|| view! {
                    <button class="btn btn--ghost btn--sm" title="Jump to the start of the event" on:click=move |_| seek(pre)>"Jump to event"</button>
                })}
                <span class="player__time">
                    {move || format!("{} / {}", format::duration(position.get() as u32), format::duration(total.get() as u32))}
                </span>
                <span class="player__spacer"></span>
                <button class="icon-btn" aria-label=move || if muted.get() { "Unmute" } else { "Mute" }
                    title=move || if muted.get() { "Unmute" } else { "Mute" } on:click=move |_| muted.update(|m| *m = !*m)>
                    {move || view! { <Icon icon=if muted.get() { I::VolumeOff } else { I::Volume } class="icon icon--sm" /> }}
                </button>
                <select class="select select--sm" aria-label="Playback speed"
                    on:change=move |ev| speed.set(event_target_value(&ev).parse().unwrap_or(1.0))>
                    {SPEEDS.iter().map(|s| view! { <option value=s.to_string() selected=*s == 1.0>{format!("{s}×")}</option> }).collect_view()}
                </select>
                {theater.map(|theater| view! {
                    <button class="icon-btn" class:icon-btn--active=theater aria-pressed=move || theater.get().to_string()
                        aria-label="Theater mode" title="Larger player (theater mode)" on:click=move |_| theater.update(|t| *t = !*t)>
                        <Icon icon=I::Theater />
                    </button>
                })}
                <button class="icon-btn" aria-label="Fullscreen" on:click=move |_| if let Some(el) = stage.get() { fullscreen::toggle(&el) }>
                    <Icon icon=I::Maximize />
                </button>
            </div>
            <p class="player__legend">
                {clip.highlight.is_some().then(|| view! {
                    <span class="legend legend--pre">"Pre-record"</span>
                    <span class="legend legend--event">"Event"</span>
                    <span class="legend legend--post">"Post-record"</span>
                })}
                {(!real).then(|| view! { <span class="muted">"Preview player — clips play here once the recording engine stores them."</span> })}
            </p>
        </div>
    }
}
