use leptos::prelude::*;
use leptos::task::spawn_local;

use super::labels;
use crate::api::{self, ConnectionProbe, ExportTarget, Topic, invalidate};
use crate::ui::{Badge, ConfirmDialog, Icon, Tone};

#[component]
pub fn TargetCard(target: ExportTarget, on_edit: Callback<()>) -> impl IntoView {
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
    // Reachable right now? Checked when this page opens (and on request),
    // never in the background; nothing is stored.
    let live = RwSignal::new(None::<ConnectionProbe>);
    let checking = RwSignal::new(false);
    let check = {
        let id = id.clone();
        move || {
            let id = id.clone();
            checking.set(true);
            spawn_local(async move {
                let probe = api::check_export_target(id).await.unwrap_or_else(|e| ConnectionProbe { ok: false, message: e.to_string(), latency_ms: None, device: None });
                live.try_set(Some(probe));
                checking.try_set(false);
            });
        }
    };
    if target.ready {
        check();
    }
    let unreachable = move || live.get().is_some_and(|p| !p.ok);
    let delete = Callback::new({ let id = id.clone(); move |_| run(Box::pin(api::delete_export_target(id.clone()))) });
    let rule = target.auto_upload;

    view! {
        <article class="dest" class:dest--problem=!target.ready>
            <span class="dest__icon"><Icon icon=labels::icon(target.kind) /></span>
            <div class="dest__main">
                <div class="dest__title">
                    <span>{target.name.clone()}</span>
                    {move || match (target.ready, unreachable()) {
                        (false, _) => view! { <Badge tone=Tone::Warning label="NEEDS ATTENTION" dot=true /> }.into_any(),
                        (true, true) => view! { <Badge tone=Tone::Warning label="UNREACHABLE" dot=true /> }.into_any(),
                        // No known problem, and the check (if done) passed.
                        (true, false) => view! { <Badge tone=Tone::Online label="READY" dot=true /> }.into_any(),
                    }}
                </div>
                <div class="dest__meta">{format!("{} · {}", labels::kind(target.kind), target.location)}</div>
                {target.problem.clone().map(|p| view! { <div class="dest__problem">{p}</div> })}
                {target.ready.then(|| view! {
                    <div class="dest__check">
                        {move || match (checking.get(), live.get()) {
                            (true, _) => view! { <span class="muted">"Checking…"</span> }.into_any(),
                            (false, Some(p)) if p.ok => view! {
                                <span class="text-online">{match p.latency_ms { Some(ms) => format!("Reachable · {ms} ms"), None => "Reachable".into() }}</span>
                            }.into_any(),
                            (false, Some(p)) => view! { <span class="dest__problem">{p.message}</span> }.into_any(),
                            (false, None) => ().into_any(),
                        }}
                        <button class="link-btn" disabled=checking on:click={let check = check.clone(); move |_| check()}>"Check again"</button>
                    </div>
                })}
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
                <button class="btn btn--secondary btn--sm" disabled=busy on:click=move |_| on_edit.run(())>"Edit"</button>
                <button class="btn btn--danger btn--sm" disabled=busy on:click=move |_| confirm.set(true)>"Remove"</button>
            </div>
            <ConfirmDialog open=confirm title="Remove destination?" confirm_label="Remove" danger=true busy error
                message=format!("Watchgrid will stop uploading to \"{}\": queued uploads are cancelled, one already running may still finish. Files already uploaded stay there.", target.name) on_confirm=delete />
        </article>
    }
}
