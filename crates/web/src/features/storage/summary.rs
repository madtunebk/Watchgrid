use leptos::prelude::*;

use crate::api::{Camera, StorageStatus};
use crate::format;
use crate::ui::{EmptyState, I, Meter};

/// Volume usage plus the cameras using the most space.
#[component]
pub fn StorageSummary(
    status: StorageStatus,
    /// Used to name cameras in the breakdown.
    cameras: Vec<Camera>,
    #[prop(default = 4)] top: usize,
) -> impl IntoView {
    if !status.available {
        return view! {
            <EmptyState icon=I::TriangleAlert title="Storage unavailable" compact=true
                text="The recording volume is not mounted. New recordings cannot be saved." />
        }
        .into_any();
    }

    let pct = status.used as f32 / status.total.max(1) as f32 * 100.0;
    let name = move |id: &str| cameras.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_else(|| id.to_string());
    let largest = status.per_camera.iter().map(|u| u.bytes).max().unwrap_or(1).max(1);

    view! {
        <div class="storage-summary">
            <div class="metric">
                <span class="metric__label mono">{status.path.clone()}</span>
                <span class="metric__value">{format!("{:.0}%", pct)}</span>
                <Meter value=pct />
                <span class="metric__hint">
                    {format!("{} free of {} · NVR uses {}", format::bytes(status.free), format::bytes(status.total), format::bytes(status.recordings_size))}
                </span>
            </div>
            {(!status.per_camera.is_empty()).then(|| view! {
                <ul class="usage-list">
                    {status.per_camera.iter().take(top).map(|u| view! {
                        <li class="usage-list__row">
                            <span class="truncate">{name(&u.camera_id)}</span>
                            <span class="usage-list__bar"><span style:width=format!("{:.1}%", u.bytes as f64 / largest as f64 * 100.0)></span></span>
                            <span class="usage-list__value">{format::bytes(u.bytes)}</span>
                        </li>
                    }).collect_view()}
                </ul>
            })}
        </div>
    }
    .into_any()
}
