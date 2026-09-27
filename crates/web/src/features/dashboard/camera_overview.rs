use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{ApiResult, Camera, CameraStatus};
use crate::features::cameras::{CameraCard, render_key};
use crate::ui::{ErrorBox, I, Icon};

/// Cards shown on the dashboard; the Cameras page lists everything.
const LIMIT: usize = 8;

/// A camera that should work but doesn't.
fn in_trouble(c: &Camera) -> bool {
    c.enabled && matches!(c.status, CameraStatus::Offline | CameraStatus::Error)
}

/// Problems first, then what is happening now: offline/error, connecting,
/// motion, recording, the rest. A camera that records normally is no incident.
fn rank(enabled: bool, status: CameraStatus, motion: bool, recording: bool) -> u8 {
    match () {
        _ if enabled && matches!(status, CameraStatus::Offline | CameraStatus::Error) => 0,
        _ if enabled && status == CameraStatus::Connecting => 1,
        _ if motion => 2,
        _ if recording => 3,
        _ => 4,
    }
}

fn attention(c: &Camera) -> u8 {
    rank(c.enabled, c.status, c.motion_active, c.recording_active)
}

#[component]
pub fn CameraOverview(cameras: LocalResource<ApiResult<Vec<Camera>>>) -> impl IntoView {
    // Problems first; stable sort keeps the configured order within each group.
    let sorted = Memo::new(move |_| {
        cameras.get().map(|r| {
            r.map(|mut list| {
                list.sort_by_key(attention);
                list
            })
        })
    });
    let loaded = Memo::new(move |_| sorted.with(|s| s.as_ref().map(|r| r.as_ref().map(|_| ()).map_err(Clone::clone))));
    let shown = Memo::new(move |_| sorted.with(|s| s.as_ref().and_then(|r| r.as_ref().ok()).map(|l| l.iter().take(LIMIT).cloned().collect::<Vec<_>>()).unwrap_or_default()));
    // (hidden, of which in trouble, total); problems are sorted first, so any
    // left out are beyond the first LIMIT.
    let more = Memo::new(move |_| {
        sorted.with(|s| {
            let list = s.as_ref().and_then(|r| r.as_ref().ok()).map(Vec::as_slice).unwrap_or_default();
            (list.len().saturating_sub(LIMIT), list.iter().skip(LIMIT).filter(|c| in_trouble(c)).count(), list.len())
        })
    });

    view! {
        <section class="section">
            <header class="section__head">
                <h2 class="section__title">"Cameras"</h2>
                <A href="/cameras" attr:class="panel__link">"Manage cameras"<Icon icon=I::ArrowRight class="icon icon--sm" /></A>
            </header>
            <div class="cam-grid">
                {move || match loaded.get() {
                    None => (0..3).map(|_| view! { <div class="cam-card cam-card--skeleton"><div class="skeleton skeleton--video"></div></div> }).collect_view().into_any(),
                    Some(Err(error)) => view! { <ErrorBox error /> }.into_any(),
                    // Keyed: a refresh rebuilds only the cards that changed.
                    Some(Ok(())) => view! {
                        <For each=move || shown.get() key=render_key let:camera><CameraCard camera /></For>
                        {move || {
                            let (hidden, hidden_trouble, total) = more.get();
                            (hidden > 0).then(|| view! {
                                <A href="/cameras" attr:class="cam-card cam-card--more">
                                    <span class="cam-card--more__count">{format!("+{hidden}")}</span>
                                    <span>{format!("View all {total} cameras")}</span>
                                    {(hidden_trouble > 0).then(|| view! {
                                        <span class="cam-card--more__trouble">{format!("{hidden_trouble} more with problems")}</span>
                                    })}
                                </A>
                            })
                        }}
                    }.into_any(),
                }}
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_come_before_busy_healthy_cameras() {
        use CameraStatus::*;
        let order = [rank(true, Offline, false, false), rank(true, Connecting, false, false), rank(true, Online, true, true), rank(true, Online, false, true), rank(true, Online, false, false)];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        assert_eq!(rank(false, Offline, false, false), 4, "a disabled camera is no problem");
        assert_eq!(rank(true, Error, false, true), 0, "an error beats recording");
    }
}
