//! Retention rules form, with a projection of how long footage is kept.
//! Used on the Storage page and in Settings → Storage.

use leptos::prelude::*;

use crate::api::{self, RetentionPolicy, StorageStatus, Topic, invalidate};
use crate::format;
use crate::ui::form::{Field, NumberInput, Switch};
use crate::ui::{SaveBar, SaveState};

const GB: u64 = 1_000_000_000;

/// Days of footage the rules allow at the current write rate.
fn projection(status: &StorageStatus, days_of_history: f64, rules: (Option<u32>, Option<u64>, Option<u64>)) -> String {
    if status.recordings_size == 0 || days_of_history <= 0.0 {
        return "No recordings yet — the projection appears once cameras record.".into();
    }
    let per_day = status.recordings_size as f64 / days_of_history;
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
    let head = format!("Recordings grow by about {} per day.", format::bytes(per_day as u64));
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
    let saved = RwSignal::new(p);
    let state = SaveState::new();

    let current = move || RetentionPolicy {
        max_age_days: age_on.get().then(|| age.get()),
        max_usage: max_on.get().then(|| max_gb.get() as u64 * GB),
        min_free: free_on.get().then(|| free_gb.get() as u64 * GB),
    };
    let dirty = Signal::derive(move || current() != saved.get());
    let days_of_history = status
        .per_camera
        .iter()
        .filter_map(|u| u.oldest)
        .min()
        .map_or(0.0, |oldest| (chrono::Utc::now() - oldest).num_seconds() as f64 / 86_400.0);
    let preview = {
        let status = status.clone();
        move || {
            let c = current();
            projection(&status, days_of_history, (c.max_age_days, c.max_usage, c.min_free))
        }
    };

    let on_save = Callback::new(move |_| {
        let policy = current();
        state.run(async move {
            api::update_retention(policy.clone()).await?;
            saved.set(policy);
            invalidate(Topic::Storage);
            Ok(())
        });
    });
    let on_revert = Callback::new(move |_| {
        let p = saved.get_untracked();
        age_on.set(p.max_age_days.is_some());
        age.set(p.max_age_days.unwrap_or(14));
        max_on.set(p.max_usage.is_some());
        max_gb.set(p.max_usage.map_or(500, |b| (b / GB) as u32));
        free_on.set(p.min_free.is_some());
        free_gb.set(p.min_free.map_or(50, |b| (b / GB) as u32));
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
            <Field label="Projection"><p class="retention__projection">{preview}</p></Field>
            <p class="note note--info">"Protected recordings are never deleted automatically, whatever these rules say. Oldest unprotected clips are removed first."</p>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
