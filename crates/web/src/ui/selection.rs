//! Selecting rows for a bulk action: a checkbox cell for each row and the
//! bar at the bottom with the count and the actions (results are toasts).

use std::collections::BTreeSet;

use leptos::prelude::*;

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
/// "clear", then the actions (`children`). With `matching` (how many match
/// the filters beyond this page) and `all_matching`, it also offers
/// "select all N matching"; ticking or unticking a row ends that.
#[component]
pub fn SelectionBar(
    selected: Selection,
    #[prop(into)] shown: Signal<Vec<String>>,
    label: &'static str,
    #[prop(optional, into)] matching: Option<Signal<u32>>,
    #[prop(optional)] all_matching: Option<RwSignal<bool>>,
    children: ChildrenFn,
) -> impl IntoView {
    let count = move || selected.with(BTreeSet::len);
    let all_shown = move || shown.with(|s| selected.with(|sel| s.iter().all(|id| sel.contains(id))));
    let everything = move || all_matching.is_some_and(|a| a.get());
    if let Some(all) = all_matching {
        Effect::new(move || {
            selected.track();
            all.set(false);
        });
    }
    // Offered once the whole page is ticked and more match than it shows.
    let more_match = move || matching.map(|m| m.get()).filter(|m| *m as usize > shown.with(Vec::len) && all_shown() && !everything());
    view! {
        <Show when=move || count() != 0>
            <div class="bulk-bar" role="toolbar" aria-label=label>
                <strong class="bulk-bar__count">{move || match (everything(), matching) {
                    (true, Some(m)) => format!("All {} matching selected", m.get()),
                    _ => format!("{} selected", count()),
                }}</strong>
                <Show when=move || !all_shown()>
                    <button class="btn btn--ghost btn--sm" on:click=move |_| selected.update(|s| s.extend(shown.get_untracked()))>
                        {move || format!("Select all shown ({})", shown.with(Vec::len))}
                    </button>
                </Show>
                {move || more_match().map(|m| view! {
                    <button class="btn btn--ghost btn--sm" on:click=move |_| if let Some(a) = all_matching { a.set(true) }>
                        {format!("Select all {m} matching")}
                    </button>
                })}
                <button class="btn btn--ghost btn--sm" on:click=move |_| selected.set(BTreeSet::new())>"Clear"</button>
                <span class="bulk-bar__spacer"></span>
                {children()}
            </div>
        </Show>
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
