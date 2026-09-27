//! Export menu for an event or a recording: copy a link (events),
//! download, or upload the clip to a configured destination (each upload
//! is its own tracked job).

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use chrono::{DateTime, Utc};

use crate::api::{self, ApiResult, ExportJob, ExportKind, ExportState, ExportTarget, Id, Topic, use_query};
use crate::clock::use_interval;
use crate::ui::{I, Icon, Popover, Tone, clipboard, use_toaster};

fn icon(kind: ExportKind) -> I {
    match kind {
        ExportKind::GoogleDrive | ExportKind::Dropbox => I::Cloud,
        ExportKind::S3 => I::Database,
        ExportKind::Nextcloud => I::Cloud,
    }
}

/// What is exported.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportSubject {
    /// An event's clip (looked up when needed).
    Event(Id),
    /// A saved recording, e.g. a continuous clip without an event.
    Recording { id: Id, camera_id: Id, start: DateTime<Utc> },
}

impl ExportSubject {
    async fn upload(self, target_id: Id) -> ApiResult<ExportJob> {
        match self {
            Self::Event(id) => api::export_event(id, target_id).await,
            Self::Recording { id, .. } => api::export_recording(id, target_id).await,
        }
    }
}

/// `<camera>_<YYYY-MM-DD_HH-MM-SS>.mp4` (local time).
fn file_name(camera_id: &str, start: DateTime<Utc>) -> String {
    format!("{camera_id}_{}.mp4", start.with_timezone(&chrono::Local).format("%Y-%m-%d_%H-%M-%S"))
}

#[component]
pub fn ExportMenu(subject: ExportSubject) -> impl IntoView {
    let open = RwSignal::new(false);
    let targets = use_query(Topic::Settings, None, api::get_export_targets);
    let jobs = RwSignal::new(Vec::<(ExportJob, String)>::new());
    let note = RwSignal::new(None::<String>);
    let toaster = use_toaster();
    // An upload leaves the list with a toast once it is done, failed, or
    // waiting for a retry (the server retries on its own).
    let outcome = move |job: &ExportJob, name: &str| -> bool {
        let why = job.message.clone().unwrap_or_default();
        match job.state {
            ExportState::Done => toaster.show(Tone::Online, format!("Clip uploaded to {name}")),
            ExportState::Failed => toaster.show(Tone::Danger, format!("Upload to {name} failed: {why}")),
            ExportState::Queued if job.message.is_some() => toaster.show(Tone::Warning, format!("Upload to {name} failed, retrying automatically: {why}")),
            ExportState::Queued | ExportState::Uploading => return false,
        }
        true
    };

    // Poll unfinished uploads.
    use_interval(Duration::from_millis(600), move || {
        for (job, _) in jobs.get_untracked() {
            if matches!(job.state, ExportState::Queued | ExportState::Uploading) {
                spawn_local(async move {
                    if let Ok(fresh) = api::get_export_job(job.id.clone()).await {
                        let Some(name) = jobs.get_untracked().into_iter().find(|(j, _)| j.id == fresh.id).map(|(_, n)| n) else { return };
                        if outcome(&fresh, &name) {
                            jobs.update(|list| list.retain(|(j, _)| j.id != fresh.id));
                        } else {
                            jobs.update(|list| {
                                if let Some(entry) = list.iter_mut().find(|(j, _)| j.id == fresh.id) {
                                    entry.0 = fresh;
                                }
                            });
                        }
                    }
                });
            }
        }
    });

    let start = {
        let subject = subject.clone();
        move |t: ExportTarget| {
            open.set(false);
            let subject = subject.clone();
            spawn_local(async move {
                match subject.upload(t.id.clone()).await {
                    // An earlier upload of this clip is reused.
                    Ok(job) if job.state == ExportState::Done => toaster.show(Tone::Online, format!("Already uploaded to {}", t.name)),
                    Ok(job) => jobs.update(|l| l.insert(0, (job, t.name.clone()))),
                    Err(e) => toaster.show(Tone::Danger, format!("Upload to {} failed: {e}", t.name)),
                }
            });
        }
    };
    let event_id = match &subject {
        ExportSubject::Event(id) => Some(id.clone()),
        ExportSubject::Recording { .. } => None,
    };
    let copy_link = {
        let event_id = event_id.clone().unwrap_or_default();
        move |_| {
            open.set(false);
            let origin = web_sys::window().and_then(|w| w.location().origin().ok()).unwrap_or_default();
            let url = format!("{origin}/events/{event_id}");
            spawn_local(async move {
                let ok = clipboard::copy(&url).await;
                note.set(Some(if ok { "Link copied to clipboard".into() } else { format!("Copy this link: {url}") }));
            });
        }
    };
    let download = {
        let subject = subject.clone();
        move |_| {
            open.set(false);
            let subject = subject.clone();
            spawn_local(async move {
                let clip = match subject {
                    ExportSubject::Recording { id, camera_id, start } => Ok(Some((id, camera_id, start))),
                    ExportSubject::Event(id) => api::get_event(id).await.map(|d| d.recording.map(|r| (r.id, d.event.camera_id, r.start_time))),
                };
                let message = match clip {
                    Ok(Some((id, camera_id, start))) => match api::recording_media_url(&id) {
                        Some(url) => {
                            let name = file_name(&camera_id, start);
                            save_as(&url, &name);
                            format!("Downloading {name}")
                        }
                        None => "The demo data has no video files.".into(),
                    },
                    Ok(None) => "This event has no video clip.".into(),
                    Err(e) => e.message,
                };
                note.set(Some(message));
            });
        }
    };
    let has_link = event_id.is_some();

    view! {
        <div class="export">
            <div class="popover-anchor">
                <button class="btn btn--secondary export__trigger" class:icon-btn--active=open on:click=move |_| open.update(|o| *o = !*o)>
                    <Icon icon=I::Upload class="icon icon--sm" />"Export"
                </button>
                <Popover open class="export__menu">
                    <div class="menu">
                        {has_link.then(|| view! {
                            <button class="menu__item" on:click=copy_link.clone()><Icon icon=I::Link class="icon icon--sm" />"Copy link to event"</button>
                        })}
                        <button class="menu__item" on:click=download.clone()><Icon icon=I::Download class="icon icon--sm" />"Download to this computer"</button>
                        <div class="menu__sep"></div>
                        <div class="menu__label">"Upload clip to"</div>
                        {
                            let start = start.clone();
                            move || match targets.get() {
                                None => view! { <div class="menu__label">"Loading…"</div> }.into_any(),
                                Some(Err(e)) => view! { <div class="menu__label">{e.to_string()}</div> }.into_any(),
                                Some(Ok(list)) if list.is_empty() => view! { <div class="menu__label">"No destinations yet"</div> }.into_any(),
                                Some(Ok(list)) => list.into_iter().map(|t| {
                                    let start = start.clone();
                                    let target = t.clone();
                                    view! {
                                        <button class="menu__item export__target" disabled=!t.ready title=t.problem.clone().unwrap_or_default()
                                            on:click=move |_| start(target.clone())>
                                            <Icon icon=icon(t.kind) class="icon icon--sm" />
                                            <span class="export__target-text">
                                                <span>{t.name.clone()}</span>
                                                {t.problem.clone().map(|p| view! { <span class="export__target-loc">{p}</span> })}
                                            </span>
                                        </button>
                                    }
                                }).collect_view().into_any(),
                            }
                        }
                        <div class="menu__sep"></div>
                        <A href="/settings/exports" attr:class="menu__item"><Icon icon=I::Settings class="icon icon--sm" />"Manage destinations"</A>
                    </div>
                </Popover>
            </div>

            {move || note.get().map(|n| view! { <p class="export__note">{n}</p> })}
            <ul class="export__jobs">
                {move || jobs.get().into_iter().map(|(job, name)| {
                    let (id, target) = (job.id.clone(), name.clone());
                    let on_cancel = Callback::new(move |_| {
                        let (id, target) = (id.clone(), target.clone());
                        spawn_local(async move {
                            match api::cancel_export(id.clone()).await {
                                Ok(_) => {
                                    jobs.update(|l| l.retain(|(j, _)| j.id != id));
                                    toaster.show(Tone::Online, format!("Upload to {target} cancelled"));
                                }
                                Err(e) => toaster.show(Tone::Danger, e.to_string()),
                            }
                        });
                    });
                    view! { <JobRow job name on_cancel /> }
                }).collect_view()}
            </ul>
        </div>
    }
}

#[component]
fn JobRow(job: ExportJob, name: String, on_cancel: Callback<()>) -> impl IntoView {
    let (text, class) = match job.state {
        ExportState::Queued => (format!("{name}: queued"), "job"),
        ExportState::Uploading => (format!("{name}: uploading {:.0}%", job.progress), "job"),
        ExportState::Done => (format!("{name}: uploaded"), "job job--done"),
        ExportState::Failed => (format!("{name}: {}", job.message.unwrap_or_else(|| "failed".into())), "job job--failed"),
    };
    view! {
        <li class=class>
            <span class="job__text">{text}</span>
            {(job.state == ExportState::Uploading || job.state == ExportState::Queued).then(|| view! {
                <span class="job__bar"><span style:width=format!("{:.0}%", job.progress)></span></span>
                <button class="icon-btn icon-btn--sm" title="Cancel upload" aria-label="Cancel upload" on:click=move |_| on_cancel.run(())>
                    <Icon icon=I::X class="icon icon--sm" />
                </button>
            })}
        </li>
    }
}

/// Let the browser download `url` (same origin) as `name`.
fn save_as(url: &str, name: &str) {
    use wasm_bindgen::JsCast;
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
    if let Some(link) = document.create_element("a").ok().and_then(|e| e.dyn_into::<web_sys::HtmlAnchorElement>().ok()) {
        link.set_href(url);
        link.set_download(name);
        link.click();
    }
}
