use leptos::prelude::*;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

use crate::api::provide_queries;
use crate::features::auth::AuthGate;
use crate::shell::Shell;
use crate::features::cameras::{CameraDetailsPage, CameraFormPage, CamerasPage};
use crate::features::dashboard::DashboardPage;
use crate::features::events::{EventDetailPage, EventsPage};
use crate::features::live::LiveViewPage;
use crate::features::notifications::NotificationsPage;
use crate::features::recordings::RecordingsPage;
use crate::features::settings::SettingsPage;
use crate::features::storage::StoragePage;
use crate::features::system::SystemPage;
use crate::shell::NotFound;
use crate::ui::{ToastHost, provide_toaster};

#[component]
pub fn App() -> impl IntoView {
    provide_queries();
    provide_toaster();

    view! {
        <AuthGate>
        <Router>
            <Routes fallback=NotFound>
                <ParentRoute path=path!("") view=Shell>
                    <Route path=path!("") view=DashboardPage />

                    <Route path=path!("cameras") view=CamerasPage />
                    // Adding a camera is a setup step: it lives under Settings.
                    <Route path=path!("settings/cameras/new") view=CameraFormPage />
                    <Route path=path!("cameras/:id/edit") view=CameraFormPage />
                    <Route path=path!("cameras/:id") view=CameraDetailsPage />
                    <Route path=path!("cameras/:id/:tab") view=CameraDetailsPage />

                    <Route path=path!("live") view=LiveViewPage />

                    <Route path=path!("events") view=EventsPage />
                    <Route path=path!("notifications") view=NotificationsPage />
                    <Route path=path!("events/:id") view=EventDetailPage />

                    <Route path=path!("recordings") view=RecordingsPage />
                    <Route path=path!("storage") view=StoragePage />
                    <Route path=path!("system") view=SystemPage />
                    <Route path=path!("settings") view=SettingsPage />
                    <Route path=path!("settings/:section") view=SettingsPage />

                    <Route path=path!("*any") view=NotFound />
                </ParentRoute>
            </Routes>
        </Router>
        </AuthGate>
        <ToastHost />
    }
}
