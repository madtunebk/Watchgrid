use leptos::prelude::*;

use crate::api::StorageStatus;
use crate::format;
use crate::ui::{Meter, Stat, Tone};

/// Capacity of the recording volume.
#[component]
pub fn VolumeStats(status: StorageStatus) -> impl IntoView {
    let pct = status.used as f32 / status.total.max(1) as f32 * 100.0;
    let nvr_pct = status.recordings_size as f32 / status.total.max(1) as f32 * 100.0;
    let tone = if pct >= 92.0 { Tone::Danger } else if pct >= 80.0 { Tone::Warning } else { Tone::Online };
    view! {
        <div class="stats">
            <Stat label="Used" value=format!("{pct:.0}%") detail=format!("{} of {}", format::bytes(status.used), format::bytes(status.total))>
                <Meter value=pct tone />
            </Stat>
            <Stat label="Available" value=format::bytes(status.free) detail=status.path.clone() />
            <Stat label="Watchgrid recordings" value=format::bytes(status.recordings_size) detail=format!("{nvr_pct:.1}% of the volume")>
                <Meter value=nvr_pct />
            </Stat>
            <Stat label="Protected" value=format::bytes(status.protected_size) detail="Never deleted automatically".to_string() />
        </div>
    }
}
