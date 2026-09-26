use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::features::clip_export::{ExportMenu, ExportSubject};
use crate::features::events::mutations;
use crate::ui::{ConfirmDialog, I, Icon};

#[component]
pub fn EventActions(
    event_id: String,
    camera_id: String,
    #[prop(into)] protected: Signal<bool>,
    /// Export and protection are about the video clip; events without one
    /// (camera offline/online, detections still recording) don't offer them.
    has_clip: bool,
    /// Called after deletion with where to go next.
    on_deleted: Callback<()>,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let confirm = RwSignal::new(false);
    let delete_error = RwSignal::new(None::<String>);

    let toggle_protect = {
        let id = event_id.clone();
        move |_| {
            let id = id.clone();
            busy.set(true);
            spawn_local(async move {
                if let Err(e) = mutations::set_protected(id, !protected.get_untracked()).await {
                    note.set(Some(e.to_string()));
                }
                busy.set(false);
            });
        }
    };
    let do_delete = Callback::new({
        let id = event_id.clone();
        move |_| {
            let id = id.clone();
            busy.set(true);
            delete_error.set(None);
            spawn_local(async move {
                match mutations::delete(id).await {
                    Ok(()) => {
                        confirm.set(false);
                        on_deleted.run(());
                    }
                    Err(e) => delete_error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    });

    view! {
        <div class="event-actions">
            {has_clip.then(|| view! {
                <ExportMenu subject=ExportSubject::Event(event_id.clone()) />
                <button class="btn" class:btn--primary=protected class:btn--secondary=move || !protected.get() disabled=busy on:click=toggle_protect
                    title="Protected events and their recordings are never deleted by retention">
                    {move || view! { <Icon icon=if protected.get() { I::Lock } else { I::LockOpen } class="icon icon--sm" /> }}
                    {move || if protected.get() { "Protected" } else { "Protect" }}
                </button>
            })}
            <A href=format!("/cameras/{camera_id}") attr:class="btn btn--secondary">
                <Icon icon=I::Cctv class="icon icon--sm" />"Open camera"
            </A>
            <button class="btn btn--danger" disabled=move || busy.get() || protected.get()
                title=move || if protected.get() { "Remove protection before deleting" } else { "" }
                on:click=move |_| confirm.set(true)>
                <Icon icon=I::Trash class="icon icon--sm" />"Delete"
            </button>
            {move || note.get().map(|n| view! { <p class="event-actions__note">{n}</p> })}
            <ConfirmDialog open=confirm title="Delete event?" confirm_label="Delete event" danger=true busy error=delete_error
                message=if has_clip {
                    "The event will be removed from the history. Its recording is kept — delete the video from Recordings if you don't need it."
                } else {
                    "The event will be deleted permanently."
                }.to_string()
                on_confirm=do_delete />
        </div>
    }
}
