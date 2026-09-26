use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use super::delete_dialog::DeleteCameraDialog;
use crate::features::cameras::mutations;
use crate::ui::{I, Icon, Popover};

/// "…" overflow menu: details, edit, enable/disable, delete.
#[component]
pub fn ActionsMenu(camera_id: String, camera_name: String, enabled: bool) -> impl IntoView {
    let open = RwSignal::new(false);
    let confirm_delete = RwSignal::new(false);
    let details = format!("/cameras/{camera_id}");
    let edit = format!("/cameras/{camera_id}/edit");
    let close = move |_| open.set(false);

    let toggle_enabled = {
        let id = camera_id.clone();
        move |_| {
            open.set(false);
            let id = id.clone();
            spawn_local(async move {
                let _ = mutations::set_enabled(id, !enabled).await;
            });
        }
    };

    view! {
        <div class="popover-anchor">
            <button class="icon-btn icon-btn--sm" class:icon-btn--active=open aria-label="More actions"
                on:click=move |_| open.update(|o| *o = !*o)>
                <Icon icon=I::Ellipsis />
            </button>
            <Popover open class="menu-popover">
                <div class="menu">
                    <A href=details.clone() attr:class="menu__item" on:click=close>
                        <Icon icon=I::Eye class="icon icon--sm" />"Open details"
                    </A>
                    <A href=edit.clone() attr:class="menu__item" on:click=close>
                        <Icon icon=I::Pencil class="icon icon--sm" />"Edit camera"
                    </A>
                    <div class="menu__sep"></div>
                    <button class="menu__item" on:click=toggle_enabled.clone()>
                        <Icon icon=I::Power class="icon icon--sm" />
                        {if enabled { "Disable camera" } else { "Enable camera" }}
                    </button>
                    <button class="menu__item menu__item--danger" on:click=move |_| { open.set(false); confirm_delete.set(true); }>
                        <Icon icon=I::Trash class="icon icon--sm" />"Delete camera"
                    </button>
                </div>
            </Popover>
            <DeleteCameraDialog open=confirm_delete camera_id camera_name />
        </div>
    }
}
