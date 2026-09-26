//! View switch, day navigation, camera picker and zoom.

use chrono::{Duration, NaiveDate};
use leptos::prelude::*;

use super::state::{State, View, ZOOMS, today};
use crate::api::Camera;
use crate::ui::{I, Icon, Popover};

#[component]
pub fn Toolbar(#[prop(into)] state: Signal<State>, #[prop(into)] cameras: Signal<Vec<Camera>>, on_change: Callback<State>) -> impl IntoView {
    let set = move |f: &dyn Fn(&mut State)| {
        let mut next = state.get_untracked();
        f(&mut next);
        on_change.run(next);
    };
    let shift = move |days: i64| set(&|s| s.date = (s.date + Duration::days(days)).min(today()));
    let zoom_step = move |dir: i32| {
        set(&|s| {
            let i = ZOOMS.iter().position(|z| *z == s.zoom).unwrap_or(0) as i32;
            s.zoom = ZOOMS[(i + dir).clamp(0, ZOOMS.len() as i32 - 1) as usize];
        })
    };
    let day_label = move || {
        let d = state.get().date;
        match (today() - d).num_days() {
            0 => format!("Today · {}", d.format("%a %-d %b")),
            1 => format!("Yesterday · {}", d.format("%a %-d %b")),
            _ => d.format("%A, %-d %B %Y").to_string(),
        }
    };
    let at_today = move || state.get().date >= today();
    let picker = RwSignal::new(false);
    let camera_label = move || {
        let s = state.get();
        match s.cameras.len() {
            0 => "All cameras".to_string(),
            1 => cameras.get().into_iter().find(|c| c.id == s.cameras[0]).map(|c| c.name).unwrap_or_else(|| "1 camera".into()),
            n => format!("{n} cameras"),
        }
    };

    view! {
        <div class="rec-toolbar">
            <div class="button-group" role="group" aria-label="View">
                <button class="button-group__btn" class:button-group__btn--active=move || state.get().view == View::Timeline
                    on:click=move |_| set(&|s| s.view = View::Timeline)>"Timeline"</button>
                <button class="button-group__btn" class:button-group__btn--active=move || state.get().view == View::Clips
                    on:click=move |_| set(&|s| s.view = View::Clips)>"Clips"</button>
            </div>

            <div class="day-nav">
                <button class="icon-btn icon-btn--sm" aria-label="Previous day" on:click=move |_| shift(-1)><Icon icon=I::ChevronLeft /></button>
                <span class="day-nav__label">{day_label}</span>
                <button class="icon-btn icon-btn--sm" aria-label="Next day" disabled=at_today
                    on:click=move |_| shift(1)><Icon icon=I::ChevronRight /></button>
                <label class="date-input">
                    <Icon icon=I::Calendar class="icon icon--sm" />
                    <input type="date" aria-label="Pick a day" prop:value=move || state.get().date.format("%Y-%m-%d").to_string()
                        max=today().format("%Y-%m-%d").to_string()
                        on:change=move |ev| if let Ok(d) = NaiveDate::parse_from_str(&event_target_value(&ev), "%Y-%m-%d") { set(&|s| s.date = d) } />
                </label>
                <Show when=move || state.get().date != today()>
                    <button class="btn btn--ghost btn--sm" on:click=move |_| set(&|s| s.date = today())>"Today"</button>
                </Show>
            </div>

            <div class="popover-anchor">
                <button class="select select--button" on:click=move |_| picker.update(|p| *p = !*p)>{camera_label}</button>
                <Popover open=picker class="camera-picker">
                    <div class="menu">
                        <label class="check-row">
                            <input type="checkbox" prop:checked=move || state.get().cameras.is_empty()
                                on:change=move |_| set(&|s| s.cameras.clear()) />
                            "All cameras"
                        </label>
                        <div class="menu__sep"></div>
                        {move || cameras.get().into_iter().map(|c| {
                            let id = c.id.clone();
                            let checked = { let id = id.clone(); move || state.get().cameras.contains(&id) };
                            view! {
                                <label class="check-row">
                                    <input type="checkbox" prop:checked=checked
                                        on:change=move |_| set(&|s| if let Some(i) = s.cameras.iter().position(|c| *c == id) { s.cameras.remove(i); } else { s.cameras.push(id.clone()); }) />
                                    {c.name}
                                </label>
                            }
                        }).collect_view()}
                    </div>
                </Popover>
            </div>

            <span class="rec-toolbar__spacer"></span>
            <Show when=move || state.get().view == View::Timeline>
                <div class="zoom" role="group" aria-label="Zoom">
                    <button class="icon-btn icon-btn--sm" aria-label="Zoom out" disabled=move || state.get().zoom == ZOOMS[0]
                        on:click=move |_| zoom_step(-1)><Icon icon=I::Minus /></button>
                    <span class="zoom__label">{move || match state.get().zoom { 1 => "24 h".to_string(), z => format!("{} h", 24 / z as u32) }}</span>
                    <button class="icon-btn icon-btn--sm" aria-label="Zoom in" disabled=move || state.get().zoom == ZOOMS[ZOOMS.len() - 1]
                        on:click=move |_| zoom_step(1)><Icon icon=I::Plus /></button>
                </div>
            </Show>
        </div>
    }
}
