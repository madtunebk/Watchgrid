//! Add a destination: pick a service, fill what it needs, test, save.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::labels;
use crate::api::{self, AutoUpload, ConnectionProbe, ExportKind, ExportTargetInput, Topic, invalidate};
use crate::ui::form::{Choice, Field, FormSection, RadioCards, TextInput};
use crate::ui::{I, Icon};

/// Google Drive / Dropbox sign-in works on the real server only once OAuth
/// is implemented; the demo (mock) build simulates it.
const OAUTH_READY: bool = cfg!(not(feature = "live-api"));

#[component]
pub fn AddDestination(on_done: Callback<()>) -> impl IntoView {
    let kind = RwSignal::new(if OAUTH_READY { ExportKind::GoogleDrive } else { ExportKind::S3 });
    let name = RwSignal::new(String::new());
    let endpoint = RwSignal::new(String::new());
    let location = RwSignal::new(String::new());
    let username = RwSignal::new(String::new());
    let secret = RwSignal::new(String::new());
    let auto = RwSignal::new(AutoUpload::Off);
    let signed_in = RwSignal::new(false);
    let probe = RwSignal::new(None::<ConnectionProbe>);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Sensible defaults when switching service.
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
    let test = move |_| {
        let i = input();
        busy.set(true);
        spawn_local(async move {
            probe.set(api::test_export_target(i).await.ok());
            busy.set(false);
        });
    };
    let save = move |_| {
        let i = input();
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::create_export_target(i).await {
                Ok(_) => {
                    invalidate(Topic::Settings);
                    on_done.run(());
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            busy.set(false);
        });
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

    view! {
        <FormSection title="Add destination">
            <Field label="Service"><RadioCards value=kind options=kinds name="export-kind" /></Field>
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
                        <Field label=Signal::derive(move || if kind.get() == ExportKind::S3 { "Secret key" } else { "App password" })>
                            <TextInput value=secret kind="password" autocomplete="new-password" />
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
                    title=move || if oauth() && !signed_in.get() { "Sign in first" } else { "" }>"Save destination"</button>
            </div>
        </FormSection>
    }
}
