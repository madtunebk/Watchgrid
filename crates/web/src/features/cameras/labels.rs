use crate::api::{Camera, CameraStatus, RecordingMode, Stream};
use crate::ui::Tone;

/// Camera connectivity as (tone, label). Deliberately separate from
/// streaming, motion and recording, which have their own indicators.
pub fn connection(cam: &Camera) -> (Tone, &'static str) {
    if !cam.enabled {
        return (Tone::Offline, "DISABLED");
    }
    match cam.status {
        CameraStatus::Online => (Tone::Online, "ONLINE"),
        CameraStatus::Connecting => (Tone::Warning, "CONNECTING"),
        CameraStatus::Offline => (Tone::Danger, "OFFLINE"),
        CameraStatus::Error => (Tone::Danger, "ERROR"),
    }
}

pub fn mode_tag(mode: RecordingMode) -> &'static str {
    match mode {
        RecordingMode::Disabled => "OFF",
        RecordingMode::Manual => "MANUAL",
        RecordingMode::Events => "EVENTS",
        RecordingMode::Continuous => "CONTINUOUS",
        RecordingMode::Scheduled => "SCHEDULED",
    }
}

/// "1080p · 25 fps"
pub fn stream_summary(s: &Stream) -> Option<String> {
    let res = s.height.map(|h| format!("{h}p"))?;
    Some(match s.fps {
        Some(fps) => format!("{res} · {fps:.0} fps"),
        None => res,
    })
}
