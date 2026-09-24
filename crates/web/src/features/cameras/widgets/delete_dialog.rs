use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::features::cameras::mutations;
use crate::ui::ConfirmDialog;

/// Confirmation for removing a camera. Recordings stay on disk.
#[component]
pub fn DeleteCameraDialog(
    open: RwSignal<bool>,
    camera_id: String,
    camera_name: String,
    /// Called after a successful delete (e.g. navigate away from its page).
    #[prop(optional)]
    on_deleted: Option<Callback<()>>,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let message = format!(
        "\"{camera_name}\" will be removed and stop recording immediately. Existing recordings are kept until retention deletes them."
    );

    let confirm = Callback::new(move |_| {
        let id = camera_id.clone();
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match mutations::delete(id).await {
                Ok(()) => {
                    busy.set(false);
                    open.set(false);
                    if let Some(cb) = on_deleted {
                        cb.run(());
                    }
                }
                Err(e) => {
                    busy.set(false);
                    error.set(Some(e.to_string()));
                }
            }
        });
    });

    view! {
        <ConfirmDialog open title="Delete camera?" message confirm_label="Delete camera" danger=true busy error on_confirm=confirm />
    }
}
