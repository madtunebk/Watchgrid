use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{self, Camera, Topic, use_query};
use crate::format;
use crate::ui::{EmptyState, I, Icon, Meter, Skeleton, Stat, async_view};

#[component]
pub fn StorageTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let id = camera.get_untracked().id;
    let storage = use_query(Topic::Storage, None, api::get_storage_status);
    view! {
        <div class="settings-tab">
            {async_view(storage, || view! { <Skeleton lines=4 height="3rem" /> }.into_any(), move |s| {
                if !s.available {
                    return view! { <EmptyState icon=I::TriangleAlert title="Storage unavailable" text="The recording volume is not mounted." /> }.into_any();
                }
                let usage = s.per_camera.iter().find(|u| u.camera_id == id).cloned();
                let bytes = usage.as_ref().map_or(0, |u| u.bytes);
                let share = bytes as f32 / s.recordings_size.max(1) as f32 * 100.0;
                let retention = s.retention.max_age_days.map_or("Unlimited".to_string(), |d| format!("{d} days"));
                view! {
                    <div class="stats">
                        <Stat label="Used by this camera" value=format::bytes(bytes)
                            detail=format!("{share:.0}% of all recordings")>
                            <Meter value=share />
                        </Stat>
                        <Stat label="Recordings" value=usage.as_ref().map_or(0, |u| u.recordings).to_string() />
                        <Stat label="Oldest recording" value=usage.as_ref().and_then(|u| u.oldest).map_or("—".to_string(), format::relative) />
                        <Stat label="Retention" value=retention detail="Protected clips are never deleted".to_string() />
                    </div>
                    <p class="note">
                        "Retention is set for all cameras on the "
                        <A href="/storage" attr:class="link">"Storage page"</A>
                        ". Per-camera retention arrives in a later version."
                        <Icon icon=I::HardDrive class="icon icon--sm note__icon" />
                    </p>
                }.into_any()
            })}
        </div>
    }
}
