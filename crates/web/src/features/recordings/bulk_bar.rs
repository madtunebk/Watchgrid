//! Bar for the selected clips: protect, unprotect or delete them together.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::bulk_delete::BulkDeleteDialog;
use super::bulk_text;
use crate::api::{self, RecordingBulkAction, RecordingBulkRequest, RecordingBulkSummary, RecordingQuery, Topic};
use crate::ui::{I, Icon, Selection, SelectionBar, Tone, use_toaster};

fn refresh() {
    for topic in [Topic::Recordings, Topic::Events, Topic::Storage, Topic::Cameras] {
        api::invalidate(topic);
    }
}

#[component]
pub fn BulkBar(
    selected: Selection,
    #[prop(into)] shown: Signal<Vec<String>>,
    /// The list's day and cameras, and how many clips it has.
    #[prop(into)]
    matching: Signal<Option<(RecordingQuery, u32)>>,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let toaster = use_toaster();
    let delete_open = RwSignal::new(false);
    let ids = Signal::derive(move || selected.get().into_iter().collect::<Vec<_>>());
    // "Select all N matching": every clip of the list, not just this page.
    let all_matching = RwSignal::new(false);
    let total = Signal::derive(move || matching.get().map_or(0, |(_, n)| n));
    let query = Signal::derive(move || all_matching.get().then(|| matching.get().map(|(q, _)| q)).flatten());
    let count = Signal::derive(move || if query.get().is_some() { total.get() as usize } else { ids.get().len() });

    let done = Callback::new(move |(action, summary): (RecordingBulkAction, RecordingBulkSummary)| {
        // Something left alone is worth a second look.
        let tone = if bulk_text::left_alone(action, &summary).is_empty() { Tone::Online } else { Tone::Warning };
        toaster.show(tone, bulk_text::did(action, &summary));
        selected.set(BTreeSet::new());
        refresh();
    });
    let run = move |action: RecordingBulkAction| {
        let req = RecordingBulkRequest { ids: ids.get_untracked(), action, matching: query.get_untracked() };
        busy.set(true);
        spawn_local(async move {
            match api::apply_recording_bulk(req).await {
                Ok(summary) => done.run((action, summary)),
                Err(e) => toaster.show(Tone::Danger, e.to_string()),
            }
            busy.set(false);
        });
    };

    view! {
        <SelectionBar selected shown label="Selected recordings" matching=total all_matching>
            <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(RecordingBulkAction::Protect)>
                <Icon icon=I::Lock class="icon icon--sm" />"Protect"
            </button>
            <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(RecordingBulkAction::Unprotect)>
                <Icon icon=I::LockOpen class="icon icon--sm" />"Unprotect"
            </button>
            <button class="btn btn--danger btn--sm" disabled=busy on:click=move |_| delete_open.set(true)>
                <Icon icon=I::Trash class="icon icon--sm" />"Delete…"
            </button>
        </SelectionBar>
        <BulkDeleteDialog open=delete_open ids matching=query count on_done=done />
    }
}
