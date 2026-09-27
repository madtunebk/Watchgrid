//! The mock camera fleet: a typical home setup covering every state the UI
//! must distinguish (online, offline, streaming, motion, recording, disabled
//! motion, single-stream camera).

use chrono::{Duration, Utc};

use crate::api::{
    Camera, CameraStatus, MotionSettings, MotionZone, MotionSource, OnvifConfig, RecordingMode, RecordingReason,
    RecordingSettings, Stream, StreamStatus,
};

struct Spec {
    id: &'static str,
    name: &'static str,
    location: &'static str,
    host: &'static str,
    online: bool,
    has_sub: bool,
    mode: RecordingMode,
    motion: bool,
    motion_active: bool,
    recording: Option<RecordingReason>,
}

const FLEET: &[Spec] = &[
    Spec { id: "cam-front", name: "Front Door", location: "Entrance", host: "192.168.1.26", online: true, has_sub: true,
        mode: RecordingMode::Events, motion: true, motion_active: true, recording: Some(RecordingReason::Motion) },
    Spec { id: "cam-driveway", name: "Driveway", location: "Outside, north", host: "192.168.1.27", online: true, has_sub: true,
        mode: RecordingMode::Events, motion: true, motion_active: false, recording: None },
    Spec { id: "cam-backyard", name: "Backyard", location: "Garden", host: "192.168.1.28", online: true, has_sub: false,
        mode: RecordingMode::Events, motion: true, motion_active: false, recording: Some(RecordingReason::Manual) },
    Spec { id: "cam-garage", name: "Garage", location: "Garage", host: "192.168.1.31", online: false, has_sub: true,
        mode: RecordingMode::Events, motion: true, motion_active: false, recording: None },
    Spec { id: "cam-living", name: "Living Room", location: "Indoor", host: "192.168.1.40", online: true, has_sub: false,
        mode: RecordingMode::Continuous, motion: false, motion_active: false, recording: Some(RecordingReason::Continuous) },
];

pub fn all() -> Vec<Camera> {
    FLEET.iter().enumerate().map(|(i, s)| build(i, s)).collect()
}

/// A large site: `n` cameras cycling through the fleet's configurations.
pub fn many(n: usize) -> Vec<Camera> {
    const AREAS: &[&str] = &["Gate", "Loading Bay", "Parking", "Hall", "Warehouse", "Office", "Stairs", "Yard"];
    (0..n)
        .map(|i| {
            let mut cam = build(i, &FLEET[i % FLEET.len()]);
            let area = AREAS[i % AREAS.len()];
            cam.id = format!("cam-{:02}", i + 1);
            cam.name = format!("{area} {:02}", i / AREAS.len() + 1);
            cam.location = area.into();
            cam.host = format!("10.0.{}.{}", 1 + i / 50, 20 + i % 200);
            cam.main_stream.url = format!("rtsp://{}:554/stream1", cam.host);
            if let Some(sub) = cam.sub_stream.as_mut() {
                sub.url = format!("rtsp://{}:554/stream2", cam.host);
            }
            // Mix of settings so capacity advice has something to say.
            if i % 7 == 3 {
                cam.recording.mode = crate::api::RecordingMode::Continuous;
            }
            if i % 5 == 1 {
                cam.motion.source = MotionSource::Software;
            }
            // Only a connected camera can detect motion or record.
            let online = cam.status == crate::api::CameraStatus::Online;
            cam.motion_active = online && cam.motion.enabled && i % 9 == 0;
            cam.recording_active = online && (cam.motion_active || i % 11 == 2);
            cam.recording_reason = cam.recording_active.then_some(crate::api::RecordingReason::Motion);
            cam
        })
        .collect()
}

fn build(index: usize, s: &Spec) -> Camera {
    let now = Utc::now();
    let motion_source = if s.id == "cam-backyard" { MotionSource::Software } else { MotionSource::Onvif };
    let stream = |path: &str, width, height, fps, bitrate| Stream {
        url: format!("rtsp://{}:554/{path}", s.host),
        status: if s.online { StreamStatus::Active } else { StreamStatus::Error },
        codec: Some("H.264".into()),
        width: Some(width),
        height: Some(height),
        fps: Some(fps),
        bitrate: Some(bitrate),
        audio_codec: (index % 2 == 0).then(|| "AAC".into()),
    };

    Camera {
        id: s.id.into(),
        name: s.name.into(),
        description: String::new(),
        location: s.location.into(),
        enabled: true,
        status: if s.online { CameraStatus::Online } else { CameraStatus::Offline },
        host: s.host.into(),
        username: "admin".into(),
        main_stream: stream("stream1", 1920, 1080, 25.0, 4096),
        sub_stream: s.has_sub.then(|| stream("stream2", 640, 360, 15.0, 512)),
        onvif: Some(OnvifConfig { url: format!("http://{}/onvif/device_service", s.host), username: "admin".into(), password: None }),
        recording: RecordingSettings { mode: s.mode, ..Default::default() },
        motion: MotionSettings { enabled: s.motion, source: motion_source, zones: zones(s.id), ..Default::default() },
        recording_active: s.recording.is_some(),
        recording_reason: s.recording,
        motion_active: s.motion_active,
        motion_fallback: false,
        last_event: None,
        storage_used: None,
        connected_since: s.online.then(|| now - Duration::hours(26 + index as i64 * 7)),
        created_at: now - Duration::days(40 - index as i64 * 5),
    }
}

/// Example detection zones (normalised coordinates) for the zone overlay.
fn zones(camera_id: &str) -> Vec<MotionZone> {
    let zone = |id: &str, name: &str, x, y, w, h, exclude| MotionZone { id: id.into(), name: name.into(), x, y, w, h, exclude };
    match camera_id {
        "cam-front" => vec![zone("z1", "Porch", 0.18, 0.35, 0.5, 0.6, false), zone("z2", "Street", 0.0, 0.0, 1.0, 0.22, true)],
        "cam-driveway" => vec![zone("z1", "Driveway", 0.1, 0.4, 0.8, 0.55, false)],
        _ => vec![],
    }
}
