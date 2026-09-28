//! Settings → Cameras: where cameras are added (a one-time setup step, so
//! it lives here and not on the everyday pages), plus a short list to reach
//! each camera's settings.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{self, Topic, use_query};
use crate::ui::form::FormSection;
use crate::ui::{ErrorBox, I, Icon, Skeleton};

#[component]
pub fn CamerasSection() -> impl IntoView {
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    view! {
        <div class="settings-tab">
            <FormSection title="Add a camera" description="Connect a camera with its RTSP address. It starts working immediately, without restarting the NVR. A substream is optional.">
                <div>
                    <A href="/settings/cameras/new" attr:class="btn btn--primary">
                        <Icon icon=I::Plus class="icon icon--sm" />"Add camera"
                    </A>
                </div>
            </FormSection>
            <FormSection title="Your cameras" description="Change a camera's connection, streams or ONVIF login; motion, recording and the rest are on its page.">
                {move || match cameras.get() {
                    None => view! { <Skeleton lines=3 /> }.into_any(),
                    Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                    Some(Ok(list)) if list.is_empty() => view! { <p class="note">"No cameras yet."</p> }.into_any(),
                    Some(Ok(list)) => view! {
                        <ul class="settings-cameras">
                            {list.into_iter().map(|c| view! {
                                <li class="settings-cameras__row">
                                    <A href=format!("/cameras/{}", c.id) attr:class="link">{c.name}</A>
                                    <span class="settings-cameras__host">{c.host}</span>
                                    <A href=format!("/cameras/{}/edit", c.id) attr:class="btn btn--secondary btn--sm">
                                        <Icon icon=I::Pencil class="icon icon--sm" />"Edit"
                                    </A>
                                </li>
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                }}
            </FormSection>
        </div>
    }
}
