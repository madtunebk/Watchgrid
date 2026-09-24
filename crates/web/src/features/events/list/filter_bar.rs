//! Filter controls. Each change produces a new `Filters` value; the page
//! turns it into a URL, so the controls themselves hold no state.

use chrono::NaiveDate;
use leptos::prelude::*;

use super::filters::{DURATIONS, Filters, HOURS, Range};
use crate::api::{Camera, EventType};
use crate::features::events::labels;
use crate::ui::{I, Icon};

#[component]
pub fn FilterBar(
    #[prop(into)] filters: Signal<Filters>,
    #[prop(into)] cameras: Signal<Vec<Camera>>,
    on_change: Callback<Filters>,
) -> impl IntoView {
    let set = move |f: &dyn Fn(&mut Filters)| {
        let mut next = filters.get_untracked();
        f(&mut next);
        on_change.run(next);
    };

    let ranges = [(Range::Today, "Today"), (Range::Yesterday, "Yesterday"), (Range::Week, "Last 7 days"), (Range::All, "All")];
    let day_value = move || match filters.get().range {
        Range::Day(d) => d.format("%Y-%m-%d").to_string(),
        _ => String::new(),
    };

    view! {
        <div class="filter-bar">
            <div class="filter-bar__row">
                <div class="button-group" role="group" aria-label="Date range">
                    {ranges.into_iter().map(|(r, label)| view! {
                        <button class="button-group__btn" class:button-group__btn--active=move || filters.get().range == r
                            on:click=move |_| set(&|f| f.range = r)>{label}</button>
                    }).collect_view()}
                </div>
                <label class="date-input" class:date-input--active=move || matches!(filters.get().range, Range::Day(_))>
                    <Icon icon=I::Calendar class="icon icon--sm" />
                    <input type="date" prop:value=day_value aria-label="Specific day"
                        on:change=move |ev| {
                            if let Ok(d) = NaiveDate::parse_from_str(&event_target_value(&ev), "%Y-%m-%d") {
                                set(&|f| f.range = Range::Day(d));
                            }
                        } />
                </label>

                <select class="select" aria-label="Camera"
                    on:change=move |ev| { let v = event_target_value(&ev); set(&|f| f.camera = (!v.is_empty()).then(|| v.clone())); }>
                    <option value="" selected=move || filters.get().camera.is_none()>"All cameras"</option>
                    {move || cameras.get().into_iter().map(|c| {
                        let id = c.id.clone();
                        view! { <option value=c.id selected=move || filters.get().camera.as_deref() == Some(id.as_str())>{c.name}</option> }
                    }).collect_view()}
                </select>

                <select class="select" aria-label="Time of day"
                    on:change=move |ev| { let v = event_target_value(&ev); set(&|f| f.hours = HOURS.iter().find(|(k, ..)| *k == v).map(|(k, ..)| *k)); }>
                    <option value="" selected=move || filters.get().hours.is_none()>"Any time"</option>
                    {HOURS.iter().map(|(key, label, _)| view! {
                        <option value=*key selected=move || filters.get().hours == Some(*key)>{*label}</option>
                    }).collect_view()}
                </select>

                <select class="select" aria-label="Minimum duration"
                    on:change=move |ev| { let v = event_target_value(&ev); set(&|f| f.min_duration = v.parse().ok()); }>
                    <option value="" selected=move || filters.get().min_duration.is_none()>"Any duration"</option>
                    {DURATIONS.iter().map(|(secs, label)| view! {
                        <option value=secs.to_string() selected=move || filters.get().min_duration == Some(*secs)>{*label}</option>
                    }).collect_view()}
                </select>

                <button class="toggle-chip" class:toggle-chip--on=move || filters.get().protected_only
                    aria-pressed=move || filters.get().protected_only.to_string()
                    on:click=move |_| set(&|f| f.protected_only = !f.protected_only)>
                    <Icon icon=I::Lock class="icon icon--sm" />"Protected"
                </button>

                <Show when=move || filters.get().is_narrowed()>
                    <button class="link-btn" on:click=move |_| set(&|f| *f = Filters { range: f.range, ..Default::default() })>
                        "Clear filters"
                    </button>
                </Show>
            </div>

            <div class="filter-bar__row" role="group" aria-label="Event types">
                {EventType::ALL.into_iter().map(|kind| view! {
                    <button class=format!("type-toggle type-toggle--{}", labels::css(kind))
                        class:type-toggle--on=move || filters.get().kinds.contains(&kind)
                        aria-pressed=move || filters.get().kinds.contains(&kind).to_string()
                        title=if labels::is_future(kind) { "Detection for this type arrives in a later version" } else { "" }
                        on:click=move |_| set(&|f| if let Some(i) = f.kinds.iter().position(|k| *k == kind) { f.kinds.remove(i); } else { f.kinds.push(kind); })>
                        <Icon icon=labels::icon(kind) class="icon icon--sm" />
                        {labels::tag(kind)}
                        {labels::is_future(kind).then(|| view! { <span class="type-toggle__future">"soon"</span> })}
                    </button>
                }).collect_view()}
            </div>
        </div>
    }
}
