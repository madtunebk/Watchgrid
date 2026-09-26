use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{ApiResult, Camera, CameraStatus};
use crate::features::cameras::CameraCard;
use crate::ui::{I, Icon, async_view};

/// Cards shown on the dashboard; the Cameras page lists everything.
const LIMIT: usize = 8;

/// Cameras needing attention come first: recording, motion, offline.
fn attention(c: &Camera) -> u8 {
    match () {
        _ if c.recording_active => 0,
        _ if c.motion_active => 1,
        _ if c.enabled && c.status != CameraStatus::Online => 2,
        _ => 3,
    }
}

#[component]
pub fn CameraOverview(cameras: LocalResource<ApiResult<Vec<Camera>>>) -> impl IntoView {
    view! {
        <section class="section">
            <header class="section__head">
                <h2 class="section__title">"Cameras"</h2>
                <A href="/cameras" attr:class="panel__link">"Manage cameras"<Icon icon=I::ArrowRight class="icon icon--sm" /></A>
            </header>
            <div class="cam-grid">
                {async_view(
                    cameras,
                    || (0..3).map(|_| view! { <div class="cam-card cam-card--skeleton"><div class="skeleton skeleton--video"></div></div> }).collect_view().into_any(),
                    |mut list| {
                        let total = list.len();
                        // Stable sort keeps the configured order within each group.
                        list.sort_by_key(attention);
                        let hidden = total.saturating_sub(LIMIT);
                        view! {
                            {list.into_iter().take(LIMIT).map(|camera| view! { <CameraCard camera /> }).collect_view()}
                            {(hidden > 0).then(|| view! {
                                <A href="/cameras" attr:class="cam-card cam-card--more">
                                    <span class="cam-card--more__count">{format!("+{hidden}")}</span>
                                    <span>{format!("View all {total} cameras")}</span>
                                </A>
                            })}
                        }
                    },
                )}
            </div>
        </section>
    }
}
