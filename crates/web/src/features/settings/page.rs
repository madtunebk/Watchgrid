use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use super::sections::{self, ALL};
use crate::api::{self, Settings, Topic, use_query};
use crate::ui::{ErrorBox, Icon, Page, Skeleton};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let params = use_params_map();
    let section = Memo::new(move |_| {
        let key = params.with(|p| p.get("section")).unwrap_or_default();
        ALL.iter().find(|s| s.key == key).unwrap_or(&ALL[0]).key
    });
    let settings = use_query(Topic::Settings, None, api::get_settings);
    let current = Memo::new(move |_| settings.get().and_then(Result::ok));
    // Sections render once settings exist; later saves flow in through `current`.
    let loaded = Memo::new(move |_| current.get().is_some());

    let title = Signal::derive(move || ALL.iter().find(|s| s.key == section.get()).map_or("Settings", |s| s.label).to_string());

    view! {
        <Page title="Settings" subtitle=title>
            <div class="settings">
                <nav class="settings__nav" aria-label="Settings sections">
                    {ALL.iter().map(|s| {
                        let key = s.key;
                        view! {
                            <a href=format!("/settings/{key}") class="settings__link"
                                class:settings__link--active=move || section.get() == key
                                aria-current=move || (section.get() == key).then_some("page")>
                                <Icon icon=s.icon class="icon icon--sm" />{s.label}
                            </a>
                        }
                    }).collect_view()}
                </nav>
                <div class="settings__content">
                    {move || {
                        if !loaded.get() {
                            return match settings.get() {
                                Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                                _ => view! { <Skeleton lines=6 height="3rem" /> }.into_any(),
                            };
                        }
                        let s: Signal<Settings> = Signal::derive(move || current.get().expect("loaded"));
                        match section.get() {
                            "cameras" => view! { <sections::CamerasSection /> }.into_any(),
                            "recording" => view! { <sections::RecordingSection settings=s /> }.into_any(),
                            "storage" => view! { <sections::StorageSection /> }.into_any(),
                            "network" => view! { <sections::NetworkSection settings=s /> }.into_any(),
                            "authentication" => view! { <sections::AuthSection settings=s /> }.into_any(),
                            "notifications" => view! { <sections::NotificationsSection settings=s /> }.into_any(),
                            "exports" => view! { <sections::ExportsSection /> }.into_any(),
                            "advanced" => view! { <sections::AdvancedSection settings=s /> }.into_any(),
                            _ => view! { <sections::GeneralSection settings=s /> }.into_any(),
                        }
                    }}
                </div>
            </div>
        </Page>
    }
}
