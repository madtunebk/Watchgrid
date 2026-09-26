//! Save / Revert bar for forms that edit server state, plus the state it shows.

use std::future::Future;

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::icons::{I, Icon};
use super::leave_guard::guard_unsaved;
use crate::api::ApiResult;

#[derive(Clone, Copy)]
pub struct SaveState {
    pub saving: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub saved: RwSignal<bool>,
}

impl SaveState {
    pub fn new() -> Self {
        Self { saving: RwSignal::new(false), error: RwSignal::new(None), saved: RwSignal::new(false) }
    }

    /// Run a save request and reflect its progress and outcome.
    pub fn run<T: 'static>(self, request: impl Future<Output = ApiResult<T>> + 'static) {
        self.saving.set(true);
        self.error.set(None);
        self.saved.set(false);
        spawn_local(async move {
            let result = request.await;
            self.saving.set(false);
            match result {
                Ok(_) => self.saved.set(true),
                Err(e) => self.error.set(Some(e.to_string())),
            }
        });
    }
}

#[component]
pub fn SaveBar(
    state: SaveState,
    #[prop(into)] dirty: Signal<bool>,
    on_save: Callback<()>,
    on_revert: Callback<()>,
    /// Shown after a successful save.
    #[prop(default = "Saved — applied live, no restart needed")]
    saved_text: &'static str,
) -> impl IntoView {
    guard_unsaved(dirty);
    view! {
        <div class="save-bar" class:save-bar--dirty=dirty>
            <span class="save-bar__status">
                {move || {
                    if let Some(e) = state.error.get() {
                        view! { <span class="text-danger"><Icon icon=I::TriangleAlert class="icon icon--sm" />{e}</span> }.into_any()
                    } else if dirty.get() {
                        view! { <span>"Unsaved changes"</span> }.into_any()
                    } else if state.saved.get() {
                        view! { <span class="text-online"><Icon icon=I::Check class="icon icon--sm" />{saved_text}</span> }.into_any()
                    } else {
                        view! { <span class="muted">"Changes apply immediately after saving"</span> }.into_any()
                    }
                }}
            </span>
            <button class="btn btn--secondary btn--sm" disabled=move || !dirty.get() || state.saving.get()
                on:click=move |_| on_revert.run(())>"Revert"</button>
            <button class="btn btn--primary btn--sm" disabled=move || !dirty.get() || state.saving.get()
                on:click=move |_| on_save.run(())>
                {move || if state.saving.get() { "Saving…" } else { "Save" }}
            </button>
        </div>
    }
}
