//! One camera's connection loop: connect, keep reading, measure, reconnect.
//! Config and credentials are re-read on every attempt, so edits apply on
//! the next reconnect (the supervisor also restarts the task on changes).

use std::time::{Duration, Instant};

use chrono::Utc;
use futures::StreamExt;
use retina::codec::CodecItem;
use watchgrid_model::CameraStatus;

use super::{Deps, backoff};
use crate::bus::BusEvent;
use crate::cameras;
use crate::rtsp::session;

/// How often fps/bitrate are recomputed.
const WINDOW: Duration = Duration::from_secs(2);

pub async fn run(deps: Deps, id: String) {
    let mut attempt = 0u32;
    loop {
        let info = match cameras::connection_info(&deps.db, &deps.credentials, &id).await {
            Ok(Some(info)) => info,
            Ok(None) => return, // camera deleted
            Err(e) => {
                tracing::warn!(camera = %id, "cannot load camera config: {e}");
                tokio::time::sleep(backoff::delay(attempt.max(1))).await;
                continue;
            }
        };
        if !info.enabled {
            deps.live.remove(&id);
            return;
        }

        deps.live.update(&id, |l| {
            l.status = CameraStatus::Connecting;
        });
        let reason = match session::open(&info.main_url, &info.username, info.password.as_deref()).await {
            Err(e) => e,
            Ok(opened) => {
                attempt = 0;
                on_online(&deps, &id, &opened.facts);
                stream_until_error(&deps, &id, opened).await
            }
        };

        attempt += 1;
        let was_online = deps.live.get(&id).is_some_and(|l| l.status == CameraStatus::Online);
        deps.live.update(&id, |l| {
            l.status = CameraStatus::Offline;
            l.connected_since = None;
            l.fps = None;
            l.bitrate = None;
            l.last_error = Some(reason.clone());
        });
        if was_online || attempt == 1 {
            tracing::warn!(camera = %id, "offline: {reason}");
            deps.bus.publish(BusEvent::CameraOffline { camera_id: id.clone(), reason: reason.clone(), at: Utc::now() });
        }
        tokio::time::sleep(backoff::delay(attempt)).await;
    }
}

fn on_online(deps: &Deps, id: &str, facts: &session::StreamFacts) {
    deps.live.update(id, |l| {
        l.status = CameraStatus::Online;
        l.connected_since = Some(Utc::now());
        l.codec = facts.video_codec.clone();
        l.width = facts.width;
        l.height = facts.height;
        l.audio_codec = facts.audio_codec.clone();
        l.last_error = None;
    });
    tracing::info!(camera = %id, codec = ?facts.video_codec, "online");
    deps.bus.publish(BusEvent::CameraOnline { camera_id: id.to_string(), at: Utc::now() });
}

/// Read frames until the stream fails; returns the reason.
async fn stream_until_error(deps: &Deps, id: &str, mut opened: session::Opened) -> String {
    let (mut frames, mut bytes, mut window_start) = (0u32, 0u64, Instant::now());
    loop {
        match opened.stream.next().await {
            None => return "the camera closed the stream".into(),
            Some(Err(e)) => return format!("stream error: {e}"),
            Some(Ok(CodecItem::VideoFrame(f))) => {
                frames += 1;
                bytes += f.data().len() as u64;
                // Frames will be handed to live view and the recorder here.
            }
            Some(Ok(_)) => {}
        }
        let elapsed = window_start.elapsed();
        if elapsed >= WINDOW {
            let secs = elapsed.as_secs_f64();
            let (fps, kbps) = (frames as f64 / secs, bytes as f64 * 8.0 / 1000.0 / secs);
            let needs_dims = deps.live.get(id).is_some_and(|l| l.width.is_none());
            let mut facts = session::StreamFacts::default();
            if needs_dims {
                session::refresh_dimensions(&mut facts, &opened.stream, opened.video);
            }
            deps.live.update(id, |l| {
                l.fps = Some(fps as f32);
                l.bitrate = Some(kbps.round() as u32);
                if facts.width.is_some() {
                    l.width = facts.width;
                    l.height = facts.height;
                }
            });
            (frames, bytes, window_start) = (0, 0, Instant::now());
        }
    }
}
