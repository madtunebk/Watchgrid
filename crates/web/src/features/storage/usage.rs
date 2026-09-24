use leptos::prelude::*;
use leptos_router::components::A;

use crate::api::{Camera, StorageStatus};
use crate::format;

/// Space used per camera, largest first.
#[component]
pub fn UsageTable(status: StorageStatus, cameras: Vec<Camera>) -> impl IntoView {
    let largest = status.per_camera.iter().map(|u| u.bytes).max().unwrap_or(1).max(1);
    let name = move |id: &str| cameras.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_else(|| format!("{id} (removed)"));
    view! {
        <div class="table-wrap">
            <table class="table">
                <thead><tr><th>"Camera"</th><th>"Usage"</th><th class="num">"Size"</th><th class="num">"Clips"</th><th>"Oldest"</th></tr></thead>
                <tbody>
                    {status.per_camera.into_iter().map(|u| {
                        let camera = name(&u.camera_id);
                        view! {
                        <tr>
                            <td><A href=format!("/cameras/{}/storage", u.camera_id) attr:class="table__primary">{camera}</A></td>
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
}
