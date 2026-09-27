//! Uploads not finished yet: queued, waiting for a retry, or running —
//! each can be cancelled (also what lets its clip be deleted right away).

use std::collections::HashMap;
use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ExportJob, ExportState, Topic, invalidate, use_query};
use crate::format;
use crate::ui::form::FormSection;
use crate::ui::{Tone, use_toaster};

fn state(job: &ExportJob) -> String {
    match (job.state, &job.message) {
        (ExportState::Uploading, _) => format!("Uploading {:.0}%", job.progress),
        (ExportState::Queued, Some(why)) => format!("Waiting to retry: {why}"),
        _ => "Queued".into(),
    }
}

#[component]
pub fn PendingUploads(#[prop(into)] targets: Signal<HashMap<String, String>>) -> impl IntoView {
    let jobs = use_query(Topic::Settings, Some(Duration::from_secs(5)), api::get_pending_exports);
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    let camera_name = move |id: &str| cameras.get().and_then(Result::ok).and_then(|l| l.into_iter().find(|c| c.id == id).map(|c| c.name)).unwrap_or_else(|| id.to_string());
    let toaster = use_toaster();
    let cancel = move |id: String| {
        spawn_local(async move {
            match api::cancel_export(id).await {
                Ok(_) => toaster.show(Tone::Online, "Upload cancelled"),
                Err(e) => toaster.show(Tone::Danger, e.to_string()),
            }
            invalidate(Topic::Settings);
        });
    };

    move || {
        let list = jobs.get().and_then(Result::ok).unwrap_or_default();
        (!list.is_empty()).then(|| view! {
            <FormSection title="Pending uploads" description="Queued, waiting for a retry, or running. A clip can't be deleted while one of its uploads is pending; cancel it first.">
                <div class="pending-list">
                    {list.into_iter().map(|job| {
                        let what = match (&job.camera_id, job.clip_start) {
                            (Some(c), Some(t)) => format!("{} · {}", camera_name(c), format::date_time(t.with_timezone(&chrono::Local))),
                            _ => "Clip".into(),
                        };
                        let to = targets.get().get(&job.target_id).cloned().unwrap_or_else(|| job.target_id.clone());
                        let id = job.id.clone();
                        view! {
                            <div class="pending">
                                <div class="pending__what">
                                    <strong>{what}</strong>
                                    <span class="muted">{format!("to {to} · {}", state(&job))}</span>
                                </div>
                                <button class="btn btn--secondary btn--sm" on:click=move |_| cancel(id.clone())>"Cancel"</button>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </FormSection>
        })
    }
}
