use leptos::prelude::*;
use leptos_router::components::A;

use crate::ui::{EmptyState, I, Icon};

/// Fresh-install state: no cameras yet. Used by the dashboard and camera list.
#[component]
pub fn NoCameras() -> impl IntoView {
    view! {
        <div class="onboarding">
            <EmptyState
                icon=I::Cctv
                title="No cameras configured"
                text="Cameras are added in Settings → Cameras, with their RTSP address. Each starts working immediately, without restarting the NVR."
            >
                <A href="/settings/cameras" attr:class="btn btn--primary">
                    <Icon icon=I::Plus class="icon icon--sm" />"Add your first camera"
                </A>
            </EmptyState>
        </div>
    }
}
