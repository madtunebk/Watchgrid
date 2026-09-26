//! One camera's health: watches the shared main feed from the media hub
//! (no RTSP session of its own), reports online/offline transitions and
//! measures fps/bitrate. The feed itself connects and reconnects; the
//! supervisor's subscription keeps it open while the camera is enabled.

use std::time::{Duration, Instant};

use chrono::Utc;
use tokio::sync::broadcast::error::RecvError;
use watchgrid_model::CameraStatus;

use super::Deps;
use crate::bus::BusEvent;
use crate::media::{FeedState, StreamKind, TrackInfo};

/// How often fps/bitrate are recomputed.
const WINDOW: Duration = Duration::from_secs(2);

pub async fn run(deps: Deps, id: String) {
    let mut sub = deps.hub.subscribe(&id, StreamKind::Main);
    let mut online = false;
    // An offline event was already published for the current outage.
    let mut outage_reported = false;
    let (mut frames, mut bytes, mut window_start) = (0u32, 0u64, Instant::now());
    sub.state.mark_changed();

    loop {
        tokio::select! {
            changed = sub.state.changed() => {
                if changed.is_err() {
                    return;
                }
                let state = sub.state.borrow_and_update().clone();
                match state {
                    FeedState::Streaming(info) => {
                        if !online {
                            online = true;
                            outage_reported = false;
                            (frames, bytes, window_start) = (0, 0, Instant::now());
                            on_online(&deps, &id, &info);
                        } else {
                            update_facts(&deps, &id, &info);
                        }
                    }
                    FeedState::Failed(reason) => {
                        let was_online = online;
                        online = false;
                        deps.live.update(&id, |l| {
                            l.status = CameraStatus::Offline;
                            l.connected_since = None;
                            l.fps = None;
                            l.bitrate = None;
                            l.last_error = Some(reason.clone());
                        });
                        if was_online || !outage_reported {
                            outage_reported = true;
                            tracing::warn!(camera = %id, "offline: {reason}");
                            deps.bus.publish(BusEvent::CameraOffline { camera_id: id.clone(), reason, at: Utc::now() });
                        }
                    }
                    FeedState::Connecting => {
                        if !online {
                            deps.live.update(&id, |l| l.status = CameraStatus::Connecting);
                        }
                    }
                }
            }
            frame = sub.frames.recv() => match frame {
                Ok(f) => {
                    frames += 1;
                    bytes += f.data.len() as u64;
                }
                // Missed frames only make one window's numbers a bit low.
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
        }
        let elapsed = window_start.elapsed();
        if online && elapsed >= WINDOW {
            let secs = elapsed.as_secs_f64();
            let (fps, kbps) = (frames as f64 / secs, bytes as f64 * 8.0 / 1000.0 / secs);
            deps.live.update(&id, |l| {
                l.fps = Some(fps as f32);
                l.bitrate = Some(kbps.round() as u32);
            });
            (frames, bytes, window_start) = (0, 0, Instant::now());
        }
    }
}

fn on_online(deps: &Deps, id: &str, info: &TrackInfo) {
    deps.live.update(id, |l| {
        l.status = CameraStatus::Online;
        l.connected_since = Some(Utc::now());
        l.last_error = None;
    });
    update_facts(deps, id, info);
    tracing::info!(camera = %id, codec = %info.codec, "online");
    deps.bus.publish(BusEvent::CameraOnline { camera_id: id.to_string(), at: Utc::now() });
}

fn update_facts(deps: &Deps, id: &str, info: &TrackInfo) {
    let codec = if info.is_h264() { "H264".to_string() } else { info.codec.split('.').next().unwrap_or(&info.codec).to_uppercase() };
    deps.live.update(id, |l| {
        l.codec = Some(codec);
        l.width = Some(info.track.width);
        l.height = Some(info.track.height);
        l.audio_codec = info.audio_codec.clone();
    });
}
