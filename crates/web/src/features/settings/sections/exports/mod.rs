//! Export destinations: where clips can be uploaded (manually or automatically).

mod form;
mod labels;
mod pending;
mod target_card;

use leptos::prelude::*;

use crate::api::{self, Topic, use_query};
use crate::ui::form::FormSection;
use crate::ui::{EmptyState, I, Icon, Skeleton, async_view};
use form::{AddDestination, EditDestination};
use pending::PendingUploads;
use target_card::TargetCard;

#[component]
pub fn ExportsSection() -> impl IntoView {
    let targets = use_query(Topic::Settings, None, api::get_export_targets);
    // One form at a time: adding, or editing the destination with this id.
    let adding = RwSignal::new(false);
    let editing = RwSignal::new(None::<api::Id>);
    let close = Callback::new(move |_| { adding.set(false); editing.set(None); });

    view! {
        <div class="settings-tab">
            <FormSection title="Destinations" description="Clips can be uploaded from any event with Export, or automatically by the rule on each destination. Credentials stay on the server.">
                {async_view(targets, || view! { <Skeleton lines=3 height="4rem" /> }.into_any(), move |list| {
                    if list.is_empty() {
                        return view! { <EmptyState icon=I::Cloud title="No destinations yet" compact=true
                            text="Add an S3-compatible bucket (MinIO, AWS, Synology C2, Backblaze B2…) or a Nextcloud / WebDAV folder to keep copies of important clips off-site." /> }.into_any();
                    }
                    view! { <div class="dest-list">{list.into_iter().map(|target| {
                        let id = target.id.clone();
                        let on_edit = Callback::new(move |_| { adding.set(false); editing.set(Some(id.clone())); });
                        view! { <TargetCard target on_edit /> }
                    }).collect_view()}</div> }.into_any()
                })}
                <Show when=move || !adding.get()>
                    <button class="btn btn--primary btn--sm" on:click=move |_| { editing.set(None); adding.set(true); }>
                        <Icon icon=I::Plus class="icon icon--sm" />"Add destination"
                    </button>
                </Show>
            </FormSection>
            <PendingUploads targets=Signal::derive(move || targets.get().and_then(Result::ok).unwrap_or_default().into_iter().map(|t| (t.id, t.name)).collect::<std::collections::HashMap<_, _>>()) />
            <Show when=move || adding.get()>
                <AddDestination on_done=close />
            </Show>
            // Keyed by id: switching to another card reloads its settings.
            {move || editing.get().map(|id| view! { <EditDestination id on_done=close /> })}
        </div>
    }
}
