//! "Previous · 101–200 of 1549 · Next" under a paged list. Hidden when
//! everything fits on one page.

use leptos::prelude::*;

#[component]
pub fn Pager(
    /// Current page, from 0.
    page: RwSignal<u32>,
    per_page: u32,
    /// Items on all pages.
    total: u32,
    /// Items on the current page.
    shown: u32,
) -> impl IntoView {
    (total > per_page).then(|| {
        let last = (total - 1) / per_page;
        let at = page.get_untracked().min(last);
        let first = at * per_page;
        let (on_first, on_last) = (at == 0, at >= last);
        let go = move |p: u32| {
            page.set(p);
            window().scroll_to_with_x_and_y(0.0, 0.0);
        };
        view! {
            <div class="load-more">
                <button class="btn btn--secondary btn--sm" disabled=on_first on:click=move |_| go(at.saturating_sub(1))>"Previous"</button>
                <span class="muted">{format!("{}–{} of {total}", first + 1, first + shown)}</span>
                <button class="btn btn--secondary btn--sm" disabled=on_last on:click=move |_| go((at + 1).min(last))>"Next"</button>
            </div>
        }
    })
}
