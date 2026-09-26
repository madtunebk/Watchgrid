use leptos::prelude::*;

use crate::api::StorageStatus;
use crate::format;
use crate::ui::{Meter, Stat, Tone};

/// Capacity of the recording volume.
#[component]
pub fn VolumeStats(status: StorageStatus) -> impl IntoView {
    let pct = status.used as f32 / status.total.max(1) as f32 * 100.0;
    // Only what lies in the current folder shares this volume.
    let nvr_pct = status.recordings_here as f32 / status.total.max(1) as f32 * 100.0;
    let elsewhere = status.recordings_size.saturating_sub(status.recordings_here);
    let nvr_detail = if elsewhere > 0 {
        format!("{nvr_pct:.1}% of the volume · {} more in earlier folders", format::bytes(elsewhere))
    } else {
        format!("{nvr_pct:.1}% of the volume")
    };
    let tone = if pct >= 92.0 { Tone::Danger } else if pct >= 80.0 { Tone::Warning } else { Tone::Online };
    view! {
        <div class="stats">
            <Stat label="Used" value=format!("{pct:.0}%") detail=format!("{} of {}", format::bytes(status.used), format::bytes(status.total))>
                <Meter value=pct tone />
            </Stat>
            <Stat label="Available" value=format::bytes(status.free) detail=status.path.clone() />
            <Stat label="Watchgrid recordings" value=format::bytes(status.recordings_here) detail=nvr_detail>
                <Meter value=nvr_pct />
            </Stat>
            <Stat label="Protected" value=format::bytes(status.protected_size) detail="Never deleted automatically".to_string() />
        </div>
    }
}
