//! Selecting rows for a bulk action: a checkbox cell for each row, the bar
//! at the bottom with the count and the actions, and the result note that
//! replaces it afterwards.

use std::collections::BTreeSet;

use leptos::prelude::*;

use super::{I, Icon};

/// Selected row ids.
pub type Selection = RwSignal<BTreeSet<String>>;

/// Checkbox cell that adds `id` to or removes it from `selected`.
#[component]
pub fn SelectCell(id: String, selected: Selection, label: &'static str) -> impl IntoView {
    let checked = {
        let id = id.clone();
        Memo::new(move |_| selected.with(|s| s.contains(&id)))
    };
    let toggle = move |_| {
        selected.update(|s| {
            if !s.remove(&id) {
                s.insert(id.clone());
            }
        })
    };
    view! {
        <label class="select-cell" title="Select">
            <input type="checkbox" aria-label=label prop:checked=checked on:change=toggle />
        </label>
    }
}

/// Bottom bar while something is selected: count, "select all shown",
/// "clear", then the actions (`children`).
#[component]
pub fn SelectionBar(selected: Selection, #[prop(into)] shown: Signal<Vec<String>>, label: &'static str, children: ChildrenFn) -> impl IntoView {
    let count = move || selected.with(BTreeSet::len);
    let all_shown = move || shown.with(|s| selected.with(|sel| s.iter().all(|id| sel.contains(id))));
    view! {
        <Show when=move || count() != 0>
            <div class="bulk-bar" role="toolbar" aria-label=label>
                <strong class="bulk-bar__count">{move || format!("{} selected", count())}</strong>
                <Show when=move || !all_shown()>
                    <button class="btn btn--ghost btn--sm" on:click=move |_| selected.update(|s| s.extend(shown.get_untracked()))>
                        {move || format!("Select all shown ({})", shown.with(Vec::len))}
                    </button>
                </Show>
                <button class="btn btn--ghost btn--sm" on:click=move |_| selected.set(BTreeSet::new())>"Clear"</button>
                <span class="bulk-bar__spacer"></span>
                {children()}
            </div>
        </Show>
    }
}

/// What the last bulk action did; a new selection dismisses it.
#[component]
pub fn ResultNote(note: RwSignal<Option<String>>, selected: Selection) -> impl IntoView {
    Effect::new(move || {
        if selected.with(|s| !s.is_empty()) {
            note.set(None);
        }
    });
    move || {
        note.get().map(|n| {
            view! {
                <div class="bulk-note" role="status">
                    <span>{n}</span>
                    <button class="icon-btn" aria-label="Dismiss" on:click=move |_| note.set(None)><Icon icon=I::X class="icon icon--sm" /></button>
                </div>
            }
        })
    }
}

/// Drop ids no longer listed (deleted elsewhere, retention) from the selection.
pub fn keep_only_shown(selected: Selection, shown: Signal<Vec<String>>) {
    Effect::new(move || {
        let ids = shown.get();
        if selected.with_untracked(|s| s.iter().any(|id| !ids.contains(id))) {
            selected.update(|s| s.retain(|id| ids.contains(id)));
        }
    });
}
