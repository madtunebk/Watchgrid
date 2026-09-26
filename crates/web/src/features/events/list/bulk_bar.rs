//! Bar for the selected events: protect, unprotect or delete them together.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::bulk_delete::BulkDeleteDialog;
use super::bulk_text;
use crate::api::{self, EventBulkAction, EventBulkRequest, EventBulkSummary, Topic};
use crate::ui::{I, Icon};

fn refresh() {
    for topic in [Topic::Events, Topic::Recordings, Topic::Storage, Topic::Cameras] {
        api::invalidate(topic);
    }
}

#[component]
pub fn BulkBar(selected: RwSignal<BTreeSet<String>>, #[prop(into)] shown: Signal<Vec<String>>) -> impl IntoView {
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let delete_open = RwSignal::new(false);
    let ids = Signal::derive(move || selected.get().into_iter().collect::<Vec<_>>());
    let count = move || selected.with(BTreeSet::len);
    let all_shown = move || shown.with(|s| s.iter().all(|id| selected.with(|sel| sel.contains(id))));

    // A new selection replaces the last result.
    Effect::new(move || {
        if count() != 0 {
            note.set(None);
        }
    });

    let done = Callback::new(move |(action, summary): (EventBulkAction, EventBulkSummary)| {
        note.set(Some(bulk_text::did(action, &summary)));
        selected.set(BTreeSet::new());
        refresh();
    });
    let run = move |action: EventBulkAction| {
        let req = EventBulkRequest { ids: ids.get_untracked(), action };
        busy.set(true);
        spawn_local(async move {
            match api::apply_event_bulk(req).await {
                Ok(summary) => done.run((action, summary)),
                Err(e) => note.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };

    view! {
        {move || note.get().map(|n| view! {
            <div class="bulk-note" role="status">
                <span>{n}</span>
                <button class="icon-btn" aria-label="Dismiss" on:click=move |_| note.set(None)><Icon icon=I::X class="icon icon--sm" /></button>
            </div>
        })}
        <Show when=move || count() != 0>
            <div class="bulk-bar" role="toolbar" aria-label="Selected events">
                <strong class="bulk-bar__count">{move || format!("{} selected", count())}</strong>
                <Show when=move || !all_shown()>
                    <button class="btn btn--ghost btn--sm" on:click=move |_| selected.update(|s| s.extend(shown.get_untracked()))>
                        {move || format!("Select all shown ({})", shown.with(Vec::len))}
                    </button>
                </Show>
                <button class="btn btn--ghost btn--sm" on:click=move |_| selected.set(BTreeSet::new())>"Clear"</button>
                <span class="bulk-bar__spacer"></span>
                <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(EventBulkAction::Protect)>
                    <Icon icon=I::Lock class="icon icon--sm" />"Protect"
                </button>
                <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(EventBulkAction::Unprotect)>
                    <Icon icon=I::LockOpen class="icon icon--sm" />"Unprotect"
                </button>
                <button class="btn btn--danger btn--sm" disabled=busy on:click=move |_| delete_open.set(true)>
                    <Icon icon=I::Trash class="icon icon--sm" />"Delete…"
                </button>
            </div>
        </Show>
        <BulkDeleteDialog open=delete_open ids on_done=done />
    }
}
