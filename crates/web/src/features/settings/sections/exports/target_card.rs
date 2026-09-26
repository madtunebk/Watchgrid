use leptos::prelude::*;
use leptos::task::spawn_local;

use super::labels;
use crate::api::{self, ExportTarget, Topic, invalidate};
use crate::ui::{Badge, ConfirmDialog, Icon, Tone};

#[component]
pub fn TargetCard(target: ExportTarget) -> impl IntoView {
    let busy = RwSignal::new(false);
    let confirm = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let id = target.id.clone();

    // Every action shows its outcome: an error stays on the card (or in the
    // remove dialog, which stays open until the server answered).
    let run = move |f: std::pin::Pin<Box<dyn std::future::Future<Output = api::ApiResult<()>>>>| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match f.await {
                Ok(()) => confirm.set(false),
                Err(e) => error.set(Some(e.to_string())),
            }
            invalidate(Topic::Settings);
            busy.set(false);
        });
    };
    let set_auto = {
        let id = id.clone();
        move |ev| {
            let key = event_target_value(&ev);
            if let Some((rule, ..)) = labels::AUTO.iter().find(|(_, k, _)| *k == key) {
                run(Box::pin(api::set_export_auto_upload(id.clone(), *rule)));
            }
        }
    };
    let reconnect = { let id = id.clone(); move |_| run(Box::pin(api::reconnect_export_target(id.clone()))) };
    let delete = Callback::new({ let id = id.clone(); move |_| run(Box::pin(api::delete_export_target(id.clone()))) });
    let rule = target.auto_upload;

    view! {
        <article class="dest" class:dest--problem=!target.ready>
            <span class="dest__icon"><Icon icon=labels::icon(target.kind) /></span>
            <div class="dest__main">
                <div class="dest__title">
                    <span>{target.name.clone()}</span>
                    {if target.ready {
                        // No known problem (checked when added, and on every upload).
                        view! { <Badge tone=Tone::Online label="READY" dot=true /> }.into_any()
                    } else {
                        view! { <Badge tone=Tone::Warning label="NEEDS ATTENTION" dot=true /> }.into_any()
                    }}
                </div>
                <div class="dest__meta">{format!("{} · {}", labels::kind(target.kind), target.location)}</div>
                {target.problem.clone().map(|p| view! { <div class="dest__problem">{p}</div> })}
                {move || error.get().filter(|_| !confirm.get()).map(|e| view! { <div class="dest__problem">{e}</div> })}
            </div>
            <label class="dest__auto">
                <span class="dest__auto-label">"Auto-upload"</span>
                <select class="select select--sm" disabled=busy on:change=set_auto>
                    {labels::AUTO.iter().map(|(r, key, label)| view! { <option value=*key selected=*r == rule>{*label}</option> }).collect_view()}
                </select>
            </label>
            <div class="dest__actions">
                {(!target.ready).then(|| view! { <button class="btn btn--primary btn--sm" disabled=busy on:click=reconnect.clone()>"Reconnect"</button> })}
                <button class="btn btn--danger btn--sm" disabled=busy on:click=move |_| confirm.set(true)>"Remove"</button>
            </div>
            <ConfirmDialog open=confirm title="Remove destination?" confirm_label="Remove" danger=true busy error
                message=format!("Watchgrid will stop uploading to \"{}\": queued uploads are cancelled, one already running may still finish. Files already uploaded stay there.", target.name) on_confirm=delete />
        </article>
    }
}
