use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use super::delete_dialog::DeleteCameraDialog;
use crate::api::session::can_change;
use crate::features::cameras::mutations;
use crate::ui::{I, Icon, Popover};

/// "…" overflow menu: details, edit, enable/disable, delete.
#[component]
pub fn ActionsMenu(camera_id: String, camera_name: String, enabled: bool) -> impl IntoView {
    if !can_change() {
        // Viewers: only the details link.
        return view! {
            <A href=format!("/cameras/{camera_id}") attr:class="icon-btn icon-btn--sm" attr:title="Open details" attr:aria-label="Open details">
                <Icon icon=I::Eye />
            </A>
        }
        .into_any();
    }
    let open = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let confirm_delete = RwSignal::new(false);
    let details = format!("/cameras/{camera_id}");
    let edit = format!("/cameras/{camera_id}/edit");
    let close = move |_| open.set(false);

    let toggle_enabled = {
        let id = camera_id.clone();
        move |_| {
            let id = id.clone();
            busy.set(true);
            error.set(None);
            spawn_local(async move {
                // The menu stays open on failure so the reason can be read.
                match mutations::set_enabled(id, !enabled).await {
                    Ok(_) => open.set(false),
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
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
                    <button class="menu__item" disabled=busy on:click=toggle_enabled.clone()>
                        <Icon icon=I::Power class="icon icon--sm" />
                        {move || match (busy.get(), enabled) {
                            (true, _) => "Working…",
                            (false, true) => "Disable camera",
                            (false, false) => "Enable camera",
                        }}
                    </button>
                    {move || error.get().map(|e| view! { <p class="menu__error">{e}</p> })}
                    <button class="menu__item menu__item--danger" on:click=move |_| { open.set(false); confirm_delete.set(true); }>
                        <Icon icon=I::Trash class="icon icon--sm" />"Delete camera"
                    </button>
                </div>
            </Popover>
            <DeleteCameraDialog open=confirm_delete camera_id camera_name />
        </div>
    }
    .into_any()
}
