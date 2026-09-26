//! Clip list view: the day's recordings, newest first.

use std::collections::HashMap;

use leptos::prelude::*;

use super::labels;
use crate::api::Recording;
use crate::format;
use crate::ui::{I, Icon, SelectCell, Selection};

#[component]
pub fn ClipList(recordings: Vec<Recording>, names: HashMap<String, String>, selected: Selection, on_open: Callback<Recording>) -> impl IntoView {
    let t = |x: chrono::DateTime<chrono::Utc>| crate::format::time_hms(x.with_timezone(&chrono::Local));
    view! {
        <div class="evt-list">
            {recordings.into_iter().rev().map(|r| {
                let camera = names.get(&r.camera_id).cloned().unwrap_or_else(|| r.camera_id.clone());
                let range = format!("{} – {}", t(r.start_time), r.end_time.map(t).unwrap_or_else(|| "now".into()));
                let events = r.event_ids.len();
                let rec = r.clone();
                let is_selected = {
                    let id = r.id.clone();
                    Memo::new(move |_| selected.with(|s| s.contains(&id)))
                };
                view! {
                    <div class="select-row" class:select-row--selected=is_selected>
                    <SelectCell id=r.id.clone() selected label="Select recording" />
                    <button class="clip-row" on:click=move |_| on_open.run(rec.clone())>
                        <span class=format!("clip-row__bar tl-seg--{}", labels::css(r.reason))></span>
                        <span class="clip-row__time">{range}</span>
                        <span class=format!("rec-chip rec-chip--{}", labels::css(r.reason))>{labels::label(r.reason)}</span>
                        <span class="clip-row__camera truncate">{camera}</span>
                        <span class="clip-row__meta">{match events { 0 => String::new(), 1 => "1 event".into(), n => format!("{n} events") }}</span>
                        <span class="clip-row__lock">{r.is_protected().then(|| view! { <Icon icon=I::Lock class="icon icon--sm" /> })}</span>
                        <span class="clip-row__num">{format::duration(r.duration)}</span>
                        <span class="clip-row__num">{format::bytes(r.file_size)}</span>
                        <Icon icon=I::ChevronRight class="icon icon--sm evt-row__chevron" />
                    </button>
                    </div>
                }
            }).collect_view()}
        </div>
    }
}
