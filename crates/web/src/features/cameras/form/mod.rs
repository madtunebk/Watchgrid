//! Add / edit camera. Everything happens in the browser; saving applies the
//! camera immediately with no NVR restart.

mod connection;
mod draft;
mod identity;
mod onvif;
mod probe;
mod streams;
mod validate;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::api::{self, CameraInput};
use crate::features::cameras::mutations;
use crate::ui::{ErrorBox, I, Icon, Page, Skeleton};
use connection::ConnectionSection;
use draft::Draft;
use identity::IdentitySection;
use onvif::OnvifSection;
use streams::StreamsSection;
use validate::Errors;

/// `/cameras/new` and `/cameras/:id/edit`.
#[component]
pub fn CameraFormPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| p.get("id"));

    match id {
        None => view! { <Page title="Add camera"><CameraForm draft=Draft::blank() camera_id=None /></Page> }.into_any(),
        Some(id) => {
            // Load once: background refreshes must not overwrite what the user is typing.
            let camera = LocalResource::new({
                let id = id.clone();
                move || api::get_camera(id.clone())
            });
            view! {
                <Page title="Edit camera">
                    {move || match camera.get() {
                        None => view! { <Skeleton lines=8 height="2.5rem" /> }.into_any(),
                        Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                        Some(Ok(cam)) => view! {
                            <CameraForm draft=Draft::from_input(&CameraInput::from(&cam)) camera_id=Some(cam.id.clone()) />
                        }.into_any(),
                    }}
                </Page>
            }
            .into_any()
        }
    }
}

#[component]
fn CameraForm(draft: Draft, camera_id: Option<String>) -> impl IntoView {
    let editing = camera_id.is_some();
    // Validate only after the first save attempt, then live on every change.
    let attempted = RwSignal::new(false);
    let errors = Signal::derive(move || {
        if !attempted.get() {
            return Errors::default();
        }
        draft.track();
        validate::check(&draft.to_input())
    });
    let saving = RwSignal::new(false);
    let save_error = RwSignal::new(None::<String>);
    let navigate = use_navigate();
    let cancel_href = camera_id.as_ref().map(|id| format!("/cameras/{id}")).unwrap_or_else(|| "/cameras".into());

    let submit = {
        let camera_id = camera_id.clone();
        move |ev: leptos::ev::SubmitEvent| {
            ev.prevent_default();
            attempted.set(true);
            let input = draft.to_input();
            if !validate::check(&input).is_empty() {
                return;
            }
            saving.set(true);
            save_error.set(None);
            let (camera_id, navigate) = (camera_id.clone(), navigate.clone());
            spawn_local(async move {
                let result = match camera_id {
                    Some(id) => mutations::update(id, input).await,
                    None => mutations::create(input).await,
                };
                saving.set(false);
                match result {
                    Ok(cam) => navigate(&format!("/cameras/{}", cam.id), Default::default()),
                    Err(e) => save_error.set(Some(e.to_string())),
                }
            });
        }
    };

    view! {
        <form class="camera-form" on:submit=submit novalidate>
            <IdentitySection draft errors />
            <ConnectionSection draft errors editing camera_id=camera_id.clone() />
            <StreamsSection draft errors camera_id=camera_id.clone() />
            <OnvifSection draft errors editing />
            {(!editing).then(|| view! {
                <p class="note note--info">
                    "New cameras record on "<strong>"motion / events"</strong>" with a 5 s pre-record and 15 s post-record buffer. You can change this in the camera's Recording tab after saving."
                </p>
            })}
            <div class="form-footer">
                {move || save_error.get().map(|e| view! { <span class="form-footer__error"><Icon icon=I::TriangleAlert class="icon icon--sm" />{e}</span> })}
                {move || (!errors.get().is_empty()).then(|| view! { <span class="form-footer__error">"Please fix the highlighted fields"</span> })}
                <span class="form-footer__spacer"></span>
                <a href=cancel_href class="btn btn--secondary">"Cancel"</a>
                <button type="submit" class="btn btn--primary" disabled=saving>
                    <Icon icon=I::Check class="icon icon--sm" />
                    {move || if saving.get() { "Saving…" } else if editing { "Save changes" } else { "Save camera" }}
                </button>
            </div>
        </form>
    }
}
