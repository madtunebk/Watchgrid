use leptos::prelude::*;
use leptos_router::components::A;

use super::save;
use crate::api::{self, Camera, Topic, use_query};
use crate::format;
use crate::ui::form::{Field, FormSection, NumberInput, Switch};
use crate::ui::{EmptyState, I, Icon, Meter, SaveBar, SaveState, Skeleton, Stat, async_view};

#[component]
pub fn StorageTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let id = camera.get_untracked().id;
    let storage = use_query(Topic::Storage, None, api::get_storage_status);

    // This camera's own age limit, on top of the global policy.
    let initial = camera.get_untracked().recording.retention_days;
    let limited = RwSignal::new(initial.is_some());
    let days = RwSignal::new(initial.unwrap_or(30));
    let state = SaveState::new();
    let draft = move || limited.get().then(|| days.get());
    let dirty = Signal::derive(move || draft() != camera.get().recording.retention_days);
    let on_save = Callback::new(move |_| {
        let value = limited.get_untracked().then(|| days.get_untracked());
        save::camera(state, &camera.get_untracked(), |i| i.recording.retention_days = value);
    });
    let on_revert = Callback::new(move |_| {
        let r = camera.get_untracked().recording.retention_days;
        limited.set(r.is_some());
        days.set(r.unwrap_or(30));
    });
    save::follow_server(draft, move || camera.get().recording.retention_days, on_revert);
    let off = Signal::derive(move || !limited.get());

    view! {
        <div class="settings-tab">
            {async_view(storage, || view! { <Skeleton lines=4 height="3rem" /> }.into_any(), move |s| {
                if !s.available {
                    return view! { <EmptyState icon=I::TriangleAlert title="Storage unavailable" text="The recording volume is not mounted." /> }.into_any();
                }
                let usage = s.per_camera.iter().find(|u| u.camera_id == id).cloned();
                let bytes = usage.as_ref().map_or(0, |u| u.bytes);
                let share = bytes as f32 / s.recordings_size.max(1) as f32 * 100.0;
                // Whichever limit is shorter wins.
                let own = camera.get().recording.retention_days;
                let retention = match (own, s.retention.max_age_days) {
                    (Some(c), Some(g)) => format!("{} days", c.min(g)),
                    (Some(d), None) | (None, Some(d)) => format!("{d} days"),
                    (None, None) => "Unlimited".to_string(),
                };
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
                        "The retention for all cameras (age, space) is set on the "
                        <A href="/storage" attr:class="link">"Storage page"</A>
                        "; a limit here only makes this camera's recordings go sooner."
                        <Icon icon=I::HardDrive class="icon icon--sm note__icon" />
                    </p>
                }.into_any()
            })}
            <FormSection title="Keep recordings" description="Protected clips are never deleted.">
                <Switch checked=limited label="Limit this camera's recordings by age"
                    description="Older recordings of this camera are deleted, whatever the global policy allows." />
                <Field label="Keep for">
                    <NumberInput value=days min=1 max=3650 suffix="days" disabled=off />
                </Field>
            </FormSection>
            <SaveBar state dirty on_save on_revert />
        </div>
    }
}
