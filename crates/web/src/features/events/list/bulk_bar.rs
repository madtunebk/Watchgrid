//! Bar for the selected events: protect, unprotect or delete them together.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::bulk_delete::BulkDeleteDialog;
use super::bulk_text;
use crate::api::{self, EventBulkAction, EventBulkRequest, EventBulkSummary, Topic};
use crate::ui::{I, Icon, Selection, SelectionBar, Tone, use_toaster};

fn refresh() {
    for topic in [Topic::Events, Topic::Recordings, Topic::Storage, Topic::Cameras] {
        api::invalidate(topic);
    }
}

#[component]
pub fn BulkBar(selected: Selection, #[prop(into)] shown: Signal<Vec<String>>) -> impl IntoView {
    let busy = RwSignal::new(false);
    let toaster = use_toaster();
    let delete_open = RwSignal::new(false);
    let ids = Signal::derive(move || selected.get().into_iter().collect::<Vec<_>>());

    let done = Callback::new(move |(action, summary): (EventBulkAction, EventBulkSummary)| {
        // Something left alone is worth a second look.
        let tone = if bulk_text::left_alone(&summary).is_empty() { Tone::Online } else { Tone::Warning };
        toaster.show(tone, bulk_text::did(action, &summary));
        selected.set(BTreeSet::new());
        refresh();
    });
    let run = move |action: EventBulkAction| {
        let req = EventBulkRequest { ids: ids.get_untracked(), action };
        busy.set(true);
        spawn_local(async move {
            match api::apply_event_bulk(req).await {
                Ok(summary) => done.run((action, summary)),
                Err(e) => toaster.show(Tone::Danger, e.to_string()),
            }
            busy.set(false);
        });
    };

    view! {
        <SelectionBar selected shown label="Selected events">
            <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(EventBulkAction::Protect)>
                <Icon icon=I::Lock class="icon icon--sm" />"Protect"
            </button>
            <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(EventBulkAction::Unprotect)>
                <Icon icon=I::LockOpen class="icon icon--sm" />"Unprotect"
            </button>
            <button class="btn btn--danger btn--sm" disabled=busy on:click=move |_| delete_open.set(true)>
                <Icon icon=I::Trash class="icon icon--sm" />"Delete…"
            </button>
        </SelectionBar>
        <BulkDeleteDialog open=delete_open ids on_done=done />
    }
}
