use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{Camera, StorageStatus};
use crate::format;
use crate::ui::{EmptyState, I};

/// Space used per camera, largest first.
#[component]
pub fn UsageTable(status: StorageStatus, cameras: Option<Vec<Camera>>) -> impl IntoView {
    if status.per_camera.is_empty() {
        return view! {
            <EmptyState icon=I::HardDrive title="No recordings yet" compact=true
                text="Space per camera appears here once cameras record." />
        }
        .into_any();
    }
    let largest = status.per_camera.iter().map(|u| u.bytes).max().unwrap_or(1).max(1);
    // (name, still configured). Unknown list: show the id, assume it exists.
    let name = move |id: &str| match &cameras {
        Some(list) => list.iter().find(|c| c.id == id).map_or_else(|| (format!("{id} (removed)"), false), |c| (c.name.clone(), true)),
        None => (id.to_string(), true),
    };
    view! {
        <div class="table-wrap">
            <table class="table">
                <thead><tr><th>"Camera"</th><th>"Usage"</th><th class="num">"Size"</th><th class="num">"Clips"</th><th>"Oldest"</th></tr></thead>
                <tbody>
                    {status.per_camera.into_iter().map(|u| {
                        let (camera, exists) = name(&u.camera_id);
                        // A removed camera's clips are still in Recordings.
                        let href = if exists { format!("/cameras/{}/storage", u.camera_id) } else { format!("/recordings?view=clips&cameras={}", u.camera_id) };
                        view! {
                        <tr>
                            <td><A href=href attr:class="table__primary">{camera}</A></td>
                            <td class="usage-cell"><span class="usage-list__bar"><span style:width=format!("{:.1}%", u.bytes as f64 / largest as f64 * 100.0)></span></span></td>
                            <td class="num mono">{format::bytes(u.bytes)}</td>
                            <td class="num mono">{u.recordings}</td>
                            <td class="muted">{u.oldest.map(format::relative).unwrap_or_else(|| "—".into())}</td>
                        </tr>
                    }}).collect_view()}
                </tbody>
            </table>
        </div>
    }
    .into_any()
}
