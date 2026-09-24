//! Reactive time: ticking clocks, intervals and calendar boundaries.

use std::time::Duration;

use chrono::{DateTime, Local};
use leptos::prelude::*;

/// Current local time, updated every `every`.
pub fn use_now(every: Duration) -> ReadSignal<DateTime<Local>> {
    let (now, set_now) = signal(Local::now());
    if let Ok(handle) = set_interval_with_handle(move || set_now.set(Local::now()), every) {
        on_cleanup(move || handle.clear());
    }
    now
}

/// Run `f` every `every` while the calling component is mounted.
pub fn use_interval(every: Duration, f: impl Fn() + 'static) {
    if let Ok(handle) = set_interval_with_handle(f, every) {
        on_cleanup(move || handle.clear());
    }
}

/// Local midnight of today, as UTC.
pub fn start_of_today() -> chrono::DateTime<chrono::Utc> {
    let midnight = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap_or_default();
    midnight.and_local_timezone(Local).earliest().map(|t| t.to_utc()).unwrap_or_else(chrono::Utc::now)
}
