//! Cameras page: every configured camera as a grid of cards or a table.

mod filter;
mod table;
mod toolbar;

use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{self, Topic, use_query};
use crate::features::cameras::widgets::{CameraCard, NoCameras};
use crate::prefs;
use crate::ui::{EmptyState, I, Icon, Page, Skeleton, async_view};
use filter::StatusFilter;
use table::CameraTable;
use toolbar::Toolbar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    List,
}

const VIEW_KEY: &str = "ui.cameras.view";

#[component]
pub fn CamerasPage() -> impl IntoView {
    let cameras = use_query(Topic::Cameras, None, api::get_cameras);
    let search = RwSignal::new(String::new());
    let status = RwSignal::new(StatusFilter::All);
    let view_mode = RwSignal::new(if prefs::get(VIEW_KEY).as_deref() == Some("list") { ViewMode::List } else { ViewMode::Grid });
    Effect::new(move || prefs::set(VIEW_KEY, if view_mode.get() == ViewMode::List { "list" } else { "grid" }));

    let subtitle = Signal::derive(move || {
        cameras.get().and_then(Result::ok).map(|c| format!("{} configured", c.len())).unwrap_or_default()
    });

    view! {
        <Page
            title="Cameras"
            subtitle=subtitle
            actions=|| view! {
                <A href="/cameras/new" attr:class="btn btn--primary btn--sm">
                    <Icon icon=I::Plus class="icon icon--sm" />"Add camera"
                </A>
            }
        >
            {async_view(cameras, || view! { <Skeleton lines=6 height="2.5rem" /> }.into_any(), move |all| {
                if all.is_empty() {
                    return view! { <NoCameras /> }.into_any();
                }
                view! {
                    <Toolbar search status view_mode />
                    {move || {
                        let shown = filter::apply(all.clone(), &search.get(), status.get());
                        if shown.is_empty() {
                            return view! { <EmptyState icon=I::Search title="No cameras match" text="Try a different search or status filter." /> }.into_any();
                        }
                        match view_mode.get() {
                            ViewMode::Grid => view! {
                                <div class="cam-grid">{shown.into_iter().map(|camera| view! { <CameraCard camera /> }).collect_view()}</div>
                            }.into_any(),
                            ViewMode::List => view! { <CameraTable cameras=shown /> }.into_any(),
                        }
                    }}
                }
                .into_any()
            })}
        </Page>
    }
}
