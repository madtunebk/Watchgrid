//! Mock camera management. Changes apply instantly, like the real NVR will
//! (no daemon restart); a new camera goes Connecting → Online on its own.

use std::time::Duration;

use chrono::Utc;
use leptos::prelude::set_timeout;

use super::db::{Db, with_db};
use super::sim::latency;
use crate::api::query::{Topic, invalidate};
use crate::api::{ApiError, ApiResult, Camera, CameraInput, CameraStatus, RecordingReason, Stream, StreamStatus};

/// Camera with live-derived fields (storage usage) filled in.
fn view(db: &Db, cam: &Camera) -> Camera {
    let mut cam = cam.clone();
    let used: u64 = db.recordings.iter().filter(|r| r.camera_id == cam.id).map(|r| r.file_size).sum();
    cam.storage_used = Some(used);
    cam
}

fn index(db: &Db, id: &str) -> ApiResult<usize> {
    db.cameras.iter().position(|c| c.id == id).ok_or_else(|| ApiError::not_found("Camera"))
}

fn validate(db: &Db, input: &CameraInput, own_id: Option<&str>) -> ApiResult<()> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::new(422, "invalid", "Camera name is required"));
    }
    if db.cameras.iter().any(|c| c.name.eq_ignore_ascii_case(name) && Some(c.id.as_str()) != own_id) {
        return Err(ApiError::conflict(format!("A camera named \"{name}\" already exists")));
    }
    Ok(())
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

pub async fn list() -> ApiResult<Vec<Camera>> {
    latency().await;
    Ok(with_db(|db| db.cameras.iter().map(|c| view(db, c)).collect()))
}

pub async fn get(id: &str) -> ApiResult<Camera> {
    latency().await;
    with_db(|db| index(db, id).map(|i| view(db, &db.cameras[i])))
}

pub async fn create(input: CameraInput) -> ApiResult<Camera> {
    latency().await;
    let cam = with_db(|db| {
        validate(db, &input, None)?;
        let mut id = format!("cam-{}", slug(&input.name));
        while db.cameras.iter().any(|c| c.id == id) {
            id.push_str("-2");
        }
        let cam = Camera {
            id,
            status: CameraStatus::Connecting,
            main_stream: Stream::unprobed(&input.main_stream_url),
            sub_stream: input.sub_stream_url.as_deref().map(Stream::unprobed),
            recording_active: false,
            recording_reason: None,
            motion_active: false,
            motion_fallback: false,
            software_motion: None,
            last_event: None,
            storage_used: Some(0),
            connected_since: None,
            created_at: Utc::now(),
            name: input.name.trim().into(),
            description: input.description,
            location: input.location,
            enabled: input.enabled,
            host: input.host,
            username: input.username,
            onvif: input.onvif.map(|o| crate::api::OnvifConfig { password: None, ..o }),
            recording: input.recording,
            motion: input.motion,
        };
        db.cameras.push(cam.clone());
        Ok::<_, ApiError>(cam)
    })?;
    simulate_connect(cam.id.clone());
    Ok(cam)
}

pub async fn update(id: &str, input: CameraInput) -> ApiResult<Camera> {
    latency().await;
    with_db(|db| {
        let i = index(db, id)?;
        validate(db, &input, Some(id))?;
        let cam = &mut db.cameras[i];
        let url_changed = cam.main_stream.url != input.main_stream_url;
        cam.name = input.name.trim().into();
        cam.description = input.description;
        cam.location = input.location;
        cam.enabled = input.enabled;
        cam.host = input.host;
        cam.username = input.username;
        if url_changed {
            cam.main_stream = Stream { url: input.main_stream_url, ..cam.main_stream.clone() };
        }
        cam.sub_stream = match (input.sub_stream_url, cam.sub_stream.take()) {
            (Some(url), Some(old)) => Some(Stream { url, ..old }),
            (Some(url), None) => Some(Stream { status: StreamStatus::Active, ..Stream::unprobed(url) }),
            (None, _) => None,
        };
        // Secrets: keep the stored password unless a new one is given.
        cam.onvif = input.onvif.map(|o| crate::api::OnvifConfig { password: None, ..o });
        cam.recording = input.recording;
        cam.motion = input.motion;
        let cam = db.cameras[i].clone();
        Ok(view(db, &cam))
    })
}

pub async fn delete(id: &str) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let i = index(db, id)?;
        db.cameras.remove(i);
        Ok(())
    })
}

pub async fn set_enabled(id: &str, enabled: bool) -> ApiResult<Camera> {
    latency().await;
    let cam = with_db(|db| {
        let i = index(db, id)?;
        let cam = &mut db.cameras[i];
        cam.enabled = enabled;
        if !enabled {
            cam.recording_active = false;
            cam.recording_reason = None;
            cam.motion_active = false;
        } else if cam.status == CameraStatus::Online {
            cam.status = CameraStatus::Connecting;
        }
        let cam = db.cameras[i].clone();
        Ok::<_, ApiError>(view(db, &cam))
    })?;
    if enabled && cam.status == CameraStatus::Connecting {
        simulate_connect(cam.id.clone());
    }
    Ok(cam)
}

/// Start/stop a manual recording. The real recorder will also stop
/// event-triggered clips on "stop"; the mock simply clears the state.
pub async fn set_manual_recording(id: &str, on: bool) -> ApiResult<Camera> {
    latency().await;
    with_db(|db| {
        let i = index(db, id)?;
        let cam = &mut db.cameras[i];
        if on && (!cam.enabled || cam.status != CameraStatus::Online) {
            return Err(ApiError::conflict(format!("{} is not online", cam.name)));
        }
        cam.recording_active = on;
        cam.recording_reason = on.then_some(RecordingReason::Manual);
        let cam = db.cameras[i].clone();
        Ok(view(db, &cam))
    })
}

/// Pretend the camera task connected and probed the stream, then push the
/// change to the UI — the same way a backend WebSocket event will.
fn simulate_connect(id: String) {
    set_timeout(
        move || {
            with_db(|db| {
                if let Some(cam) = db.cameras.iter_mut().find(|c| c.id == id) {
                    cam.status = CameraStatus::Online;
                    cam.connected_since = Some(Utc::now());
                    let probed = |s: &mut Stream, w, h, fps, kbps| {
                        *s = Stream { status: StreamStatus::Active, codec: Some("H.264".into()), width: Some(w), height: Some(h), fps: Some(fps), bitrate: Some(kbps), audio_codec: Some("AAC".into()), ..s.clone() };
                    };
                    probed(&mut cam.main_stream, 1920, 1080, 25.0, 4096);
                    if let Some(sub) = cam.sub_stream.as_mut() {
                        probed(sub, 640, 360, 15.0, 512);
                    }
                }
            });
            invalidate(Topic::Cameras);
            invalidate(Topic::System);
        },
        Duration::from_millis(2500),
    );
}
