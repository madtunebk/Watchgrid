//! Add or edit a destination: pick a service, fill what it needs, test, save.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::labels;
use crate::api::{self, AutoUpload, ConnectionProbe, ExportKind, ExportTargetInput, ExportTargetSettings, Id, Topic, invalidate};
use crate::ui::form::{Choice, Field, FormSection, RadioCards, TextInput};
use crate::ui::{I, Icon, Skeleton, async_view};

/// Google Drive / Dropbox sign-in works on the real server only once OAuth
/// is implemented; the demo (mock) build simulates it.
const OAUTH_READY: bool = cfg!(not(feature = "live-api"));

#[component]
pub fn AddDestination(on_done: Callback<()>) -> impl IntoView {
    view! { <DestinationForm saved=None on_done /> }
}

/// Loads the saved settings, then shows them in the form.
#[component]
pub fn EditDestination(id: Id, on_done: Callback<()>) -> impl IntoView {
    let settings = LocalResource::new({ let id = id.clone(); move || api::get_export_target_settings(id.clone()) });
    async_view(settings, || view! { <Skeleton lines=4 height="3rem" /> }.into_any(), move |s| {
        view! { <DestinationForm saved=Some((id.clone(), s)) on_done /> }
    })
}

/// `saved`: the destination being edited. Its service stays; its secret is
/// never shown and is kept unless a new one is typed.
#[component]
fn DestinationForm(saved: Option<(Id, ExportTargetSettings)>, on_done: Callback<()>) -> impl IntoView {
    let editing = saved.as_ref().map(|(id, _)| id.clone());
    let has_secret = saved.as_ref().is_some_and(|(_, s)| s.has_secret);
    let kind = RwSignal::new(match &saved {
        Some((_, s)) => s.kind,
        None if OAUTH_READY => ExportKind::GoogleDrive,
        None => ExportKind::S3,
    });
    let field = |f: fn(&ExportTargetSettings) -> &String| RwSignal::new(saved.as_ref().map(|(_, s)| f(s).clone()).unwrap_or_default());
    let name = field(|s| &s.name);
    let endpoint = field(|s| &s.endpoint);
    let location = field(|s| &s.location);
    let username = field(|s| &s.username);
    let secret = RwSignal::new(String::new());
    let auto = RwSignal::new(saved.as_ref().map_or(AutoUpload::Off, |(_, s)| s.auto_upload));
    let signed_in = RwSignal::new(editing.is_some());
    let probe = RwSignal::new(None::<ConnectionProbe>);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Sensible defaults when switching service (only when adding).
    if editing.is_none() {
        Effect::new(move || {
            let k = kind.get();
            name.set(labels::kind(k).split(" (").next().unwrap_or("").to_string());
            location.set(match k {
                ExportKind::GoogleDrive | ExportKind::Dropbox => "/Watchgrid".into(),
                ExportKind::S3 => "nvr-backup/clips".into(),
                ExportKind::Nextcloud => "/Cameras".into(),
            });
            signed_in.set(false);
            probe.set(None);
        });
    }

    let input = move || ExportTargetInput {
        name: name.get(),
        kind: kind.get(),
        endpoint: endpoint.get().trim().into(),
        location: location.get().trim().into(),
        username: username.get().trim().into(),
        secret: Some(secret.get()).filter(|s| !s.is_empty()),
        auto_upload: auto.get(),
    };
    let sign_in = move |_| {
        busy.set(true);
        // Real flow: open the provider's consent page, receive the token on the server.
        set_timeout(move || { signed_in.set(true); busy.set(false); }, std::time::Duration::from_millis(900));
    };
    // A test result is about the details it tested: editing them clears it.
    Effect::new(move || {
        input();
        probe.set(None);
    });
    let test = {
        let editing = editing.clone();
        move |_| {
            let i = input();
            let editing = editing.clone();
            busy.set(true);
            spawn_local(async move {
                let result = match editing {
                    Some(id) => api::test_saved_export_target(id, i).await,
                    None => api::test_export_target(i).await,
                };
                probe.set(Some(result.unwrap_or_else(|e| ConnectionProbe { ok: false, message: e.to_string(), latency_ms: None, device: None })));
                busy.set(false);
            });
        }
    };
    let save = {
        let editing = editing.clone();
        move |_| {
            let i = input();
            let editing = editing.clone();
            busy.set(true);
            error.set(None);
            spawn_local(async move {
                let result = match editing {
                    Some(id) => api::update_export_target(id, i).await,
                    None => api::create_export_target(i).await,
                };
                match result {
                    Ok(_) => {
                        invalidate(Topic::Settings);
                        on_done.run(());
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
                busy.set(false);
            });
        }
    };
    let oauth = move || labels::uses_oauth(kind.get());
    let can_save = move || !busy.get() && (!oauth() || signed_in.get());

    let oauth_choice = |c: Choice<ExportKind>| if OAUTH_READY { c } else { c.tag("Soon").disabled_because("Sign-in with this service arrives in a later version") };
    let kinds = vec![
        oauth_choice(Choice::new(ExportKind::GoogleDrive, "Google Drive").describe("Sign in with Google; clips go to a Drive folder.")),
        Choice::new(ExportKind::S3, "S3 / MinIO").describe("Any S3-compatible bucket: AWS, MinIO, Wasabi, Backblaze B2."),
        Choice::new(ExportKind::Nextcloud, "Nextcloud / WebDAV").describe("Nextcloud with an app password, or any WebDAV folder URL."),
        oauth_choice(Choice::new(ExportKind::Dropbox, "Dropbox").describe("Sign in with Dropbox; clips go to an app folder.")),
    ];
    let secret_hint = if has_secret { "Leave empty to keep the saved one. Needed again if the server URL changes." } else { "" };

    view! {
        <FormSection title=if editing.is_some() { "Edit destination" } else { "Add destination" }>
            {match editing.is_some() {
                // The service of a saved destination can't change.
                true => view! { <Field label="Service" hint="To use another service, add a new destination.">
                    <TextInput value=RwSignal::new(labels::kind(kind.get_untracked()).to_string()) disabled=true />
                </Field> }.into_any(),
                false => view! { <Field label="Service"><RadioCards value=kind options=kinds name="export-kind" /></Field> }.into_any(),
            }}
            <div class="form-grid">
                <Field label="Name"><TextInput value=name placeholder="Google Drive" /></Field>
                <Field label=Signal::derive(move || if kind.get() == ExportKind::S3 { "Bucket / prefix" } else { "Folder" })>
                    <TextInput value=location mono=true />
                </Field>
            </div>
            {move || if oauth() {
                view! {
                    <div class="oauth">
                        <button class="btn btn--secondary" disabled=busy on:click=sign_in>
                            <Icon icon=I::ExternalLink class="icon icon--sm" />
                            {move || format!("Sign in with {}", labels::kind(kind.get()))}
                        </button>
                        {move || signed_in.get().then(|| view! { <span class="text-online"><Icon icon=I::Check class="icon icon--sm" />"Signed in (mock)"</span> })}
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="form-grid">
                        <div class="form-grid__wide">
                            <Field label="Server URL"><TextInput value=endpoint mono=true
                                placeholder=Signal::derive(move || if kind.get() == ExportKind::S3 { "https://minio.local:9000".to_string() } else { "https://cloud.example.com".to_string() }) /></Field>
                        </div>
                        <Field label=Signal::derive(move || if kind.get() == ExportKind::S3 { "Access key" } else { "Username" })><TextInput value=username /></Field>
                        <Field label=Signal::derive(move || if kind.get() == ExportKind::S3 { "Secret key" } else { "App password" }) hint=secret_hint>
                            <TextInput value=secret kind="password" autocomplete="new-password"
                                placeholder=if has_secret { "Saved (unchanged)" } else { "" } />
                        </Field>
                    </div>
                }.into_any()
            }}
            <Field label="Upload automatically">
                <select class="select" on:change=move |ev| {
                    let key = event_target_value(&ev);
                    if let Some((rule, ..)) = labels::AUTO.iter().find(|(_, k, _)| *k == key) { auto.set(*rule); }
                }>
                    {labels::AUTO.iter().map(|(r, key, label)| view! { <option value=*key selected=move || auto.get() == *r>{*label}</option> }).collect_view()}
                </select>
            </Field>
            {move || probe.get().map(|p| view! {
                <p class=if p.ok { "text-online" } else { "text-danger" }>{p.message}</p>
            })}
            {move || error.get().map(|e| view! { <p class="text-danger">{e}</p> })}
            <div class="form-actions">
                <button class="btn btn--secondary" disabled=busy on:click=test>"Test"</button>
                <span class="toolbar__spacer"></span>
                <button class="btn btn--secondary" on:click=move |_| on_done.run(())>"Cancel"</button>
                <button class="btn btn--primary" disabled=move || !can_save() on:click=save
                    title=move || if oauth() && !signed_in.get() { "Sign in first" } else { "" }>
                    {if editing.is_some() { "Save changes" } else { "Save destination" }}
                </button>
            </div>
        </FormSection>
    }
}
