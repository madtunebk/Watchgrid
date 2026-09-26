use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::api::{self, Topic, invalidate, use_query};
use crate::features::storage::RetentionForm;
use crate::format;
use crate::ui::form::{Field, FormSection, TextInput};
use crate::ui::{EmptyState, ErrorBox, I, Skeleton};

#[component]
pub fn StorageSection() -> impl IntoView {
    let storage = use_query(Topic::Storage, None, api::get_storage_status);
    // Build the form once, so refreshes don't reset edits.
    let first = Memo::new(move |_| storage.get().map(|r| r.map(|s| s.available)));
    view! {
        <div class="settings-tab">
            {move || match first.get() {
                None => view! { <Skeleton lines=4 height="3rem" /> }.into_any(),
                Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                Some(Ok(false)) => view! { <EmptyState icon=I::TriangleAlert title="Storage unavailable" text="The recording volume is not mounted." /> }.into_any(),
                Some(Ok(true)) => {
                    let status = storage.get_untracked().and_then(Result::ok).expect("loaded");
                    let capacity = format!("{} free of {}", format::bytes(status.free), format::bytes(status.total));
                    let folder = status.path.clone();
                    view! {
                        <FormSection title="Recording location" description="Where new clips are written. Existing clips stay where they are and keep playing.">
                            <FolderField current=folder />
                            <dl class="kv kv--wide">
                                <div><dt>"Capacity"</dt><dd>{capacity}</dd></div>
                            </dl>
                            <p class="note">"Watchgrid can only use a folder it may already write to. To prepare a new one (e.g. another disk), run "<code>"sudo watchgrid storage set-path <folder>"</code>" on the server. Usage per camera is on the "<A href="/storage" attr:class="link">"Storage page"</A>"."</p>
                        </FormSection>
                        <FormSection title="Retention"><RetentionForm status /></FormSection>
                    }.into_any()
                }
            }}
        </div>
    }
}

/// The recordings folder with its own Save (applies immediately).
#[component]
fn FolderField(current: String) -> impl IntoView {
    let value = RwSignal::new(current.clone());
    let saved = RwSignal::new(current);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let done = RwSignal::new(false);
    let save = move |_| {
        let path = value.get_untracked().trim().to_string();
        busy.set(true);
        error.set(None);
        done.set(false);
        spawn_local(async move {
            match api::set_recordings_path(path.clone()).await {
                Ok(()) => {
                    saved.set(path);
                    done.set(true);
                    invalidate(Topic::Storage);
                }
                Err(e) => error.set(Some(e.message)),
            }
            busy.set(false);
        });
    };
    let unchanged = move || value.get().trim() == saved.get();
    view! {
        <Field label="Recordings folder" error=Signal::derive(move || error.get())>
            <div class="inline-field">
                <TextInput value mono=true placeholder="/volume1/watchgrid" />
                <button class="btn btn--primary btn--sm" disabled=move || busy.get() || unchanged() on:click=save>
                    {move || if busy.get() { "Checking…" } else { "Use this folder" }}
                </button>
            </div>
        </Field>
        {move || done.get().then(|| view! { <p class="note note--ok">"New recordings now go to this folder."</p> })}
    }
}
