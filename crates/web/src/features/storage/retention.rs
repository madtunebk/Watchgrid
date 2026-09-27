//! Retention rules form (Storage page), with a projection of how long
//! footage is kept and, before saving, what the new rules delete right away.

use leptos::prelude::*;

use crate::api::{self, DEFAULT_EVENT_DAYS, RetentionPolicy, RetentionPreview, StorageStatus, Topic, invalidate};
use crate::format;
use crate::ui::form::{Field, NumberInput, Switch};
use crate::ui::{ConfirmDialog, SaveBar, SaveState};

const GB: u64 = 1_000_000_000;

/// The form edits whole GB; a stored limit that isn't (set through the API,
/// e.g. 1.5 GB) is kept as it is unless its GB value was changed.
fn bytes(stored: Option<u64>, gb: u32) -> u64 {
    match stored {
        Some(b) if b / GB == u64::from(gb) => b,
        _ => u64::from(gb) * GB,
    }
}

/// The confirmation text, or `None` when nothing would be deleted now.
fn deletes_now(p: &RetentionPreview) -> Option<String> {
    let mut parts = Vec::new();
    if p.recordings > 0 {
        parts.push(format!("{} recording{} ({})", p.recordings, if p.recordings == 1 { "" } else { "s" }, format::bytes(p.bytes)));
    }
    if p.events > 0 {
        parts.push(format!("{} event{} from the history", p.events, if p.events == 1 { "" } else { "s" }));
    }
    (!parts.is_empty()).then(|| format!("These rules delete {} right away. Protected recordings are kept. This cannot be undone.", parts.join(" and ")))
}

/// "the last 7 days", "the last 5 hours"
fn window(hours: u32) -> String {
    match hours {
        h if h >= 48 => format!("the last {} days", (h as f32 / 24.0).round()),
        h => format!("the last {h} hour{}", if h == 1 { "" } else { "s" }),
    }
}

/// Days of footage the rules allow at the rate measured lately.
fn projection(status: &StorageStatus, rules: (Option<u32>, Option<u64>, Option<u64>)) -> String {
    let rate = match status.write_rate {
        None => return "Not enough recordings yet — the projection appears after about an hour of recording.".into(),
        Some(r) if r.bytes_per_day == 0 => return format!("Nothing was recorded in {}, so there is no rate to project from.", window(r.window_hours)),
        Some(r) => r,
    };
    let per_day = rate.bytes_per_day as f64;
    let mut limits: Vec<(f64, &str)> = Vec::new();
    if let Some(d) = rules.0 {
        limits.push((d as f64, "age"));
    }
    if let Some(max) = rules.1 {
        limits.push((max as f64 / per_day, "maximum usage"));
    }
    if let Some(min_free) = rules.2 {
        let room = (status.free + status.recordings_size).saturating_sub(min_free);
        limits.push((room as f64 / per_day, "free-space reserve"));
    }
    let settle = if rate.window_hours < 24 { " The estimate settles after a full day." } else { "" };
    let head = format!("Recordings grow by about {} per day (measured over {}).{settle}", format::bytes(per_day as u64), window(rate.window_hours));
    match limits.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)) {
        Some((days, why)) => format!("{head} With these rules footage is kept for about {:.0} days (limited by {why}).", days.floor()),
        None => format!("{head} Without limits the disk fills in about {:.0} days, then recording stops.", status.free as f64 / per_day),
    }
}

#[component]
pub fn RetentionForm(status: StorageStatus) -> impl IntoView {
    let p = status.retention.clone();
    let age_on = RwSignal::new(p.max_age_days.is_some());
    let age = RwSignal::new(p.max_age_days.unwrap_or(14));
    let max_on = RwSignal::new(p.max_usage.is_some());
    let max_gb = RwSignal::new(p.max_usage.map_or(500, |b| (b / GB) as u32));
    let free_on = RwSignal::new(p.min_free.is_some());
    let free_gb = RwSignal::new(p.min_free.map_or(50, |b| (b / GB) as u32));
    let events_on = RwSignal::new(p.event_history_days.is_some());
    let events_days = RwSignal::new(p.event_history_days.unwrap_or(365));
    let saved = RwSignal::new(p);
    let state = SaveState::new();

    let current = move || {
        let was = saved.get();
        RetentionPolicy {
            max_age_days: age_on.get().then(|| age.get()),
            max_usage: max_on.get().then(|| bytes(was.max_usage, max_gb.get())),
            min_free: free_on.get().then(|| bytes(was.min_free, free_gb.get())),
            event_history_days: events_on.get().then(|| events_days.get()),
        }
    };
    // What the history keeps while it has no rule of its own.
    let events_default = move || match age_on.get() {
        true => format!("Same as recordings: {} days", age.get()),
        false => format!("Same as recordings: without an age rule, {DEFAULT_EVENT_DAYS} days"),
    };
    let dirty = Signal::derive(move || current() != saved.get());
    let preview = {
        let status = status.clone();
        move || {
            let c = current();
            projection(&status, (c.max_age_days, c.max_usage, c.min_free))
        }
    };

    let store = move |policy: RetentionPolicy| {
        state.run(async move {
            api::update_retention(policy.clone()).await?;
            saved.set(policy);
            invalidate(Topic::Storage);
            Ok(())
        });
    };
    // Save asks first what the rules would delete now; if anything, confirm.
    let confirm_open = RwSignal::new(false);
    let pending = RwSignal::new(None::<(RetentionPolicy, String)>);
    let on_save = Callback::new(move |_| {
        let policy = current();
        state.run(async move {
            let preview = api::preview_retention(policy.clone()).await?;
            match deletes_now(&preview) {
                Some(text) => {
                    pending.set(Some((policy, text)));
                    confirm_open.set(true);
                }
                None => {
                    api::update_retention(policy.clone()).await?;
                    saved.set(policy);
                    invalidate(Topic::Storage);
                }
            }
            Ok(())
        });
    });
    let on_confirm = Callback::new(move |_| {
        if let Some((policy, _)) = pending.get_untracked() {
            confirm_open.set(false);
            store(policy);
        }
    });
    let confirm_text = Signal::derive(move || pending.get().map(|(_, t)| t).unwrap_or_default());
    let on_revert = Callback::new(move |_| {
        let p = saved.get_untracked();
        age_on.set(p.max_age_days.is_some());
        age.set(p.max_age_days.unwrap_or(14));
        max_on.set(p.max_usage.is_some());
        max_gb.set(p.max_usage.map_or(500, |b| (b / GB) as u32));
        free_on.set(p.min_free.is_some());
        free_gb.set(p.min_free.map_or(50, |b| (b / GB) as u32));
        events_on.set(p.event_history_days.is_some());
        events_days.set(p.event_history_days.unwrap_or(365));
    });

    view! {
        <div class="retention">
            <div class="retention__rule">
                <Switch checked=age_on label="Delete recordings older than" />
                <NumberInput value=age min=1 max=3650 suffix="days" disabled=Signal::derive(move || !age_on.get()) />
            </div>
            <div class="retention__rule">
                <Switch checked=max_on label="Maximum storage usage" />
                <NumberInput value=max_gb min=1 max=100_000 suffix="GB" disabled=Signal::derive(move || !max_on.get()) />
            </div>
            <div class="retention__rule">
                <Switch checked=free_on label="Always keep free on the volume" />
                <NumberInput value=free_gb min=1 max=100_000 suffix="GB" disabled=Signal::derive(move || !free_on.get()) />
            </div>
            <div class="retention__rule">
                <Switch checked=events_on label="Keep event history for" />
                <NumberInput value=events_days min=1 max=3650 suffix="days" disabled=Signal::derive(move || !events_on.get()) />
            </div>
            <p class="note">{move || if events_on.get() {
                "Events older than this leave the history, even when their video is still kept.".to_string()
            } else {
                events_default()
            }}</p>
            <Field label="Projection"><p class="retention__projection">{preview}</p></Field>
            <p class="note note--info">"Protected recordings are never deleted automatically, whatever these rules say. Oldest unprotected clips are removed first."</p>
            <SaveBar state dirty on_save on_revert />
            <ConfirmDialog open=confirm_open title="Delete recordings now?" confirm_label="Save and delete" danger=true
                message=confirm_text on_confirm />
        </div>
    }
}
