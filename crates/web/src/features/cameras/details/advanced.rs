use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::Camera;
use crate::features::cameras::mutations;
use crate::features::cameras::widgets::DeleteCameraDialog;
use crate::format;
use crate::ui::form::FormSection;
use crate::ui::{I, Icon};

#[component]
pub fn AdvancedTab(#[prop(into)] camera: Signal<Camera>) -> impl IntoView {
    let cam = camera.get_untracked();
    let confirm_delete = RwSignal::new(false);
    let toggling = RwSignal::new(false);
    let navigate = use_navigate();
    let on_deleted = Callback::new(move |_| navigate("/cameras", Default::default()));

    let toggle = move |_| {
        let c = camera.get_untracked();
        toggling.set(true);
        spawn_local(async move {
            let _ = mutations::set_enabled(c.id, !c.enabled).await;
            toggling.set(false);
        });
    };

    let edit_href = format!("/cameras/{}/edit", cam.id);
    let (del_id, del_name) = (cam.id.clone(), cam.name.clone());
    let rows = vec![
        ("Camera ID", cam.id.clone()),
        ("Added", format::relative(cam.created_at)),
        ("Username", cam.username.clone()),
        ("ONVIF", cam.onvif.as_ref().map_or("Not configured".into(), |o| o.url.clone())),
    ];

    view! {
        <div class="settings-tab">
            <FormSection title="Identifiers">
                <dl class="kv kv--wide">
                    {rows.into_iter().map(|(k, v)| view! { <div><dt>{k}</dt><dd class="mono">{v}</dd></div> }).collect_view()}
                </dl>
                <A href=edit_href attr:class="btn btn--secondary btn--sm">
                    <Icon icon=I::Pencil class="icon icon--sm" />"Edit connection & streams"
                </A>
            </FormSection>

            <FormSection title="Availability" description="A disabled camera keeps its settings and recordings but stops streaming, detecting and recording.">
                <button class="btn btn--secondary" disabled=toggling on:click=toggle>
                    <Icon icon=I::Power class="icon icon--sm" />
                    {move || if camera.get().enabled { "Disable camera" } else { "Enable camera" }}
                </button>
            </FormSection>

            <section class="danger-zone">
                <div>
                    <h2 class="danger-zone__title">"Delete camera"</h2>
                    <p class="danger-zone__text">"Removes the camera immediately. Existing recordings stay until retention removes them."</p>
                </div>
                <button class="btn btn--danger" on:click=move |_| confirm_delete.set(true)>
                    <Icon icon=I::Trash class="icon icon--sm" />"Delete camera"
                </button>
                <DeleteCameraDialog open=confirm_delete camera_id=del_id camera_name=del_name on_deleted />
            </section>
        </div>
    }
}
