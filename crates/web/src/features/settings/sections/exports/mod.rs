//! Export destinations: where clips can be uploaded (manually or automatically).

mod add_form;
mod labels;
mod target_card;

use leptos::prelude::*;

use crate::api::{self, Topic, use_query};
use crate::ui::form::FormSection;
use crate::ui::{EmptyState, I, Icon, Skeleton, async_view};
use add_form::AddDestination;
use target_card::TargetCard;

#[component]
pub fn ExportsSection() -> impl IntoView {
    let targets = use_query(Topic::Settings, None, api::get_export_targets);
    let adding = RwSignal::new(false);

    view! {
        <div class="settings-tab">
            <FormSection title="Destinations" description="Clips can be uploaded from any event with Export, or automatically by the rule on each destination. Credentials stay on the server.">
                {async_view(targets, || view! { <Skeleton lines=3 height="4rem" /> }.into_any(), |list| {
                    if list.is_empty() {
                        return view! { <EmptyState icon=I::Cloud title="No destinations yet" compact=true
                            text="Add Google Drive, an S3 / MinIO bucket, Nextcloud or Dropbox to keep copies of important clips off-site." /> }.into_any();
                    }
                    view! { <div class="dest-list">{list.into_iter().map(|target| view! { <TargetCard target /> }).collect_view()}</div> }.into_any()
                })}
                <Show when=move || !adding.get()>
                    <button class="btn btn--primary btn--sm" on:click=move |_| adding.set(true)>
                        <Icon icon=I::Plus class="icon icon--sm" />"Add destination"
                    </button>
                </Show>
            </FormSection>
            <Show when=move || adding.get()>
                <AddDestination on_done=Callback::new(move |_| adding.set(false)) />
            </Show>
        </div>
    }
}
