//! Bar for the selected notifications: mark read / unread, delete.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, NotificationBulkAction, NotificationBulkRequest, Topic};
use crate::ui::{ConfirmDialog, I, Icon, ResultNote, Selection, SelectionBar};

fn plural(n: u32) -> String {
    format!("{n} notification{}", if n == 1 { "" } else { "s" })
}

#[component]
pub fn BulkBar(selected: Selection, #[prop(into)] shown: Signal<Vec<String>>) -> impl IntoView {
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let confirm = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let run = move |action: NotificationBulkAction| {
        let req = NotificationBulkRequest { ids: selected.get_untracked().into_iter().collect(), action };
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::notifications_bulk(req).await {
                Ok(r) => {
                    let what = plural(r.changed);
                    note.set(Some(match action {
                        NotificationBulkAction::Read => format!("Marked {what} as read."),
                        NotificationBulkAction::Unread => format!("Marked {what} as unread."),
                        NotificationBulkAction::Delete => format!("Deleted {what}."),
                    }));
                    confirm.set(false);
                    selected.set(BTreeSet::new());
                    api::invalidate(Topic::Notifications);
                }
                Err(e) if action == NotificationBulkAction::Delete => error.set(Some(e.to_string())),
                Err(e) => note.set(Some(e.to_string())),
            }
            busy.set(false);
        });
    };
    let message = Signal::derive(move || format!("{} will be removed from the list. Events and recordings are not affected.", plural(selected.with(BTreeSet::len) as u32)));

    view! {
        <ResultNote note selected />
        <SelectionBar selected shown label="Selected notifications">
            <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(NotificationBulkAction::Read)>
                <Icon icon=I::Check class="icon icon--sm" />"Mark read"
            </button>
            <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| run(NotificationBulkAction::Unread)>
                <Icon icon=I::Bell class="icon icon--sm" />"Mark unread"
            </button>
            <button class="btn btn--danger btn--sm" disabled=busy on:click=move |_| confirm.set(true)>
                <Icon icon=I::Trash class="icon icon--sm" />"Delete…"
            </button>
        </SelectionBar>
        <ConfirmDialog open=confirm title="Delete notifications?" confirm_label="Delete" danger=true busy error message
            on_confirm=Callback::new(move |_| run(NotificationBulkAction::Delete)) />
    }
}
