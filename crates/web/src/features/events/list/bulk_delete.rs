//! Confirmation for deleting the selected events: events only, or events
//! and their videos. Shows the server's preview of exactly what goes.

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::bulk_text;
use crate::api::{self, EventBulkAction, EventBulkRequest, EventBulkSummary};

#[component]
pub fn BulkDeleteDialog(open: RwSignal<bool>, ids: Signal<Vec<String>>, on_done: Callback<(EventBulkAction, EventBulkSummary)>) -> impl IntoView {
    let with_video = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let action = move || if with_video.get() { EventBulkAction::DeleteWithVideo } else { EventBulkAction::Delete };
    let preview = LocalResource::new(move || {
        let (open, req) = (open.get(), EventBulkRequest { ids: ids.get(), action: action() });
        async move {
            if !open {
                return None;
            }
            Some(api::preview_event_bulk(req).await)
        }
    });
    let close = move || {
        if !busy.get_untracked() {
            open.set(false);
            with_video.set(false);
            error.set(None);
        }
    };
    let esc = window_event_listener(ev::keydown, move |e| if e.key() == "Escape" { close() });
    on_cleanup(move || esc.remove());

    let ready = move || preview.get().flatten().and_then(Result::ok).filter(|s| s.events > 0);
    let confirm = move |_| {
        let req = EventBulkRequest { ids: ids.get_untracked(), action: action() };
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            let act = req.action;
            match api::apply_event_bulk(req).await {
                Ok(summary) => {
                    busy.set(false);
                    close();
                    on_done.run((act, summary));
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <Show when=move || open.get()>
            <div class="modal-backdrop" on:click=move |_| close()></div>
            <div class="modal" role="alertdialog" aria-modal="true" aria-label="Delete events">
                <h2 class="modal__title">{move || { let n = ids.with(Vec::len); format!("Delete {n} event{}?", if n == 1 { "" } else { "s" }) }}</h2>
                <div class="bulk-choice">
                    <label class="bulk-choice__option">
                        <input type="radio" name="bulk-delete" prop:checked=move || !with_video.get() on:change=move |_| with_video.set(false) />
                        <span><strong>"Events only"</strong><small>"Remove them from the history; the videos stay in Recordings."</small></span>
                    </label>
                    <label class="bulk-choice__option">
                        <input type="radio" name="bulk-delete" prop:checked=move || with_video.get() on:change=move |_| with_video.set(true) />
                        <span><strong>"Events and their videos"</strong><small>"Also delete recordings nothing else needs. Shared and protected ones are kept."</small></span>
                    </label>
                </div>
                <div class="modal__text bulk-preview">
                    {move || match preview.get().flatten() {
                        None => view! { <p>"Checking…"</p> }.into_any(),
                        Some(Err(e)) => view! { <p class="modal__error">{e.to_string()}</p> }.into_any(),
                        Some(Ok(s)) => {
                            let act = action();
                            let permanent = act == EventBulkAction::DeleteWithVideo && s.recordings > 0;
                            view! {
                                <p class="bulk-preview__main">{bulk_text::will(act, &s)}</p>
                                {bulk_text::left_alone(&s).into_iter().map(|l| view! { <p>{l}</p> }).collect_view()}
                                {permanent.then(|| view! { <p class="bulk-preview__warn">"The video files cannot be recovered."</p> })}
                            }.into_any()
                        }
                    }}
                </div>
                {move || error.get().map(|e| view! { <p class="modal__error">{e}</p> })}
                <div class="modal__actions">
                    <button class="btn btn--secondary" disabled=busy on:click=move |_| close()>"Cancel"</button>
                    <button class="btn btn--danger-solid" disabled=move || busy.get() || ready().is_none() on:click=confirm>
                        {move || if busy.get() { "Deleting…" } else if with_video.get() { "Delete events and videos" } else { "Delete events" }}
                    </button>
                </div>
            </div>
        </Show>
    }
}
