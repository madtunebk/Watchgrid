//! One camera's health: watches the shared substream feed from the media
//! hub (no RTSP session of its own) and reports online/offline transitions.
//! The substream is cheap, so it stays open while the camera is enabled —
//! a camera that is unplugged is noticed within seconds. The main stream is
//! only opened by what needs it (a single-camera view, a recording); a
//! camera without a substream is watched on its main stream. Each feed
//! measures its own frame rate and bitrate (the hub keeps them).

use chrono::Utc;
use tokio::sync::broadcast::error::RecvError;
use watchgrid_model::CameraStatus;

use super::Deps;
use crate::bus::BusEvent;
use crate::media::{FeedState, StreamKind, TrackInfo};

pub async fn run(deps: Deps, id: String) {
    let mut sub = deps.hub.subscribe(&id, StreamKind::Sub);
    let mut online = false;
    // An offline event was already published for the current outage.
    let mut outage_reported = false;
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
                            on_online(&deps, &id, &info);
                        }
                    }
                    FeedState::Failed(reason) => {
                        let was_online = online;
                        online = false;
                        deps.live.update(&id, |l| {
                            l.status = CameraStatus::Offline;
                            l.connected_since = None;
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
            // Frames aren't needed here; reading them only notices the end.
            frame = sub.frames.recv() => match frame {
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
        }
    }
}

fn on_online(deps: &Deps, id: &str, info: &TrackInfo) {
    deps.live.update(id, |l| {
        l.status = CameraStatus::Online;
        l.connected_since = Some(Utc::now());
        l.last_error = None;
    });
    tracing::info!(camera = %id, codec = %info.codec, "online");
    deps.bus.publish(BusEvent::CameraOnline { camera_id: id.to_string(), at: Utc::now() });
}
