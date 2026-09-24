use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{self, Topic, use_query};
use crate::features::storage::RetentionForm;
use crate::format;
use crate::ui::form::FormSection;
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
                    let path = status.path.clone();
                    let capacity = format!("{} free of {}", format::bytes(status.free), format::bytes(status.total));
                    view! {
                        <FormSection title="Recording location" description="Where clips are written.">
                            <dl class="kv kv--wide">
                                <div><dt>"Path"</dt><dd class="mono">{path}</dd></div>
                                <div><dt>"Capacity"</dt><dd>{capacity}</dd></div>
                            </dl>
                            <p class="note">"Moving recordings to another volume arrives with the storage backend. See usage per camera on the "<A href="/storage" attr:class="link">"Storage page"</A>"."</p>
                        </FormSection>
                        <FormSection title="Retention"><RetentionForm status /></FormSection>
                    }.into_any()
                }
            }}
        </div>
    }
}
