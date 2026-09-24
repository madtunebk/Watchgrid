use std::time::Duration;

use leptos::prelude::*;

use super::retention::RetentionForm;
use super::usage::UsageTable;
use super::volume::VolumeStats;
use crate::api::{self, Topic, use_query};
use crate::ui::{EmptyState, ErrorBox, I, Page, Panel, Skeleton};

#[component]
pub fn StoragePage() -> impl IntoView {
    let storage = use_query(Topic::Storage, Some(Duration::from_secs(60)), api::get_storage_status);
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);

    // Rebuild only when availability changes, so the retention form keeps its edits.
    let loaded = Memo::new(move |_| storage.get().map(|r| r.map(|s| s.available)));

    view! {
        <Page title="Storage" subtitle=Signal::derive(move || storage.get().and_then(Result::ok).map(|s| s.path).unwrap_or_default())>
            {move || match loaded.get() {
                None => view! { <Skeleton lines=4 height="4rem" /> }.into_any(),
                Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                Some(Ok(false)) => view! {
                    <EmptyState icon=I::TriangleAlert title="Storage unavailable"
                        text="The recording volume is not mounted or not writable. Live view keeps working, but nothing can be recorded until it is back." />
                }.into_any(),
                Some(Ok(true)) => {
                    let status = storage.get_untracked().and_then(Result::ok).expect("loaded");
                    view! {
                        {move || storage.get().and_then(Result::ok).map(|status| view! { <VolumeStats status /> })}
                        <div class="storage-layout">
                            <Panel title="Usage by camera">
                                {move || {
                                    let list = cameras.get().and_then(Result::ok).unwrap_or_default();
                                    storage.get().and_then(Result::ok).map(|status| view! { <UsageTable status cameras=list /> })
                                }}
                            </Panel>
                            <Panel title="Retention">
                                <RetentionForm status />
                            </Panel>
                        </div>
                    }.into_any()
                }
            }}
        </Page>
    }
}
