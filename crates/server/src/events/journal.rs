//! The event journal: turns bus transitions into durable events.
//!
//! camera offline  → opens an outage event (idempotent while it lasts)
//! camera online   → closes the outage and records a "back online" event
//! camera stopped  → closes the outage (disabled or deleted)
//! recording start → opens a recording event linked to the recording
//! recording stop  → closes it with the recording's real end time
//! detection       → opens/closes a motion/person/… event (origin "onvif",
//!                   or "software" for Watchgrid's own motion detection);
//!                   shorter than the camera's "minimum event" → dropped;
//!                   one starting within `repo::DETECTION_MERGE_SECS` of the
//!                   previous one's end continues it
//! missed messages → detections caught up from the combined detection
//!                   state (see [`catch_up`]); other kinds are closed by
//!                   their next transition or at the next start

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;

use sqlx::PgPool;
use tokio::sync::broadcast::error::RecvError;
use watchgrid_model::{EventType, RecordingReason};

use super::repo;
use crate::bus::{Bus, BusEvent};
use crate::motion::{Detections, Source};

/// Subscribe now (so nothing published after this call is missed) and
/// process events in the background.
pub fn start(db: PgPool, bus: Bus, detections: Arc<Detections>) {
    let mut events = bus.subscribe();
    tokio::spawn(async move {
        match repo::close_stale(&db).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("closed {n} recording event(s) left open by the previous run"),
            Err(e) => tracing::warn!("cannot close stale events: {e}"),
        }
        let mut links = Links::default();
        loop {
            match events.recv().await {
                Ok(event) => match handle(&db, &mut links, &event).await {
                    Ok(true) => bus.publish(BusEvent::EventsChanged),
                    Ok(false) => {}
                    Err(e) => tracing::warn!(?event, "cannot store event: {e}"),
                },
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("event journal missed {n} bus messages; catching up detections");
                    match catch_up(&db, &links, &detections).await {
                        Ok(true) => bus.publish(BusEvent::EventsChanged),
                        Ok(false) => {}
                        Err(e) => tracing::warn!("cannot catch up detections: {e}"),
                    }
                }
                Err(RecvError::Closed) => return,
            }
        }
    });
}

/// After missed bus messages: make the open detection events match what the
/// cameras see now. A lost end would leave an event open ("LIVE") forever;
/// a lost start would leave a detection without an event. Times are "now":
/// the real moment was in the missed messages.
pub async fn catch_up(db: &PgPool, links: &Links, detections: &Detections) -> sqlx::Result<bool> {
    let now = Utc::now();
    let open = repo::open_detections(db).await?;
    let seen = detections.snapshot();
    let mut changed = false;
    for (camera, kind) in &open {
        if !seen.iter().any(|(c, k, _)| c == camera && k == kind) {
            let min = crate::cameras::repo_get(db, camera).await.ok().flatten().map_or(0, |c| c.recording.min_event_seconds);
            changed |= repo::close_detection(db, camera, *kind, now, min).await?;
        }
    }
    for (camera, kind, source) in &seen {
        if !open.iter().any(|(c, k)| c == camera && k == kind) {
            let (text, origin) = match source {
                Source::Software => ("Software motion", "software"),
                Source::Camera => ("ONVIF: detection (start caught up after missed messages)", "onvif"),
            };
            changed |= repo::open_detection(db, camera, *kind, now, text, links.0.get(camera).map(String::as_str), origin).await?;
        }
    }
    Ok(changed)
}

/// Event recordings in progress, per camera: detections that start while
/// one runs are linked to its clip.
#[derive(Default)]
pub struct Links(HashMap<String, String>);

/// Store one transition. Returns whether anything changed.
pub async fn handle(db: &PgPool, links: &mut Links, event: &BusEvent) -> sqlx::Result<bool> {
    match event {
        BusEvent::CameraOffline { camera_id, reason, at } => {
            repo::open(db, camera_id, EventType::CameraOffline, *at, &format!("Supervisor: {reason}"), None).await
        }
        BusEvent::CameraOnline { camera_id, at } => {
            let was_down = repo::close(db, camera_id, EventType::CameraOffline, *at).await?;
            if was_down {
                repo::instant(db, camera_id, EventType::CameraOnline, *at, "Supervisor: stream restored").await?;
            }
            Ok(was_down)
        }
        BusEvent::CameraStopped { camera_id, at } => repo::close(db, camera_id, EventType::CameraOffline, *at).await,
        BusEvent::RecordingStarted { camera_id, recording_id, reason, at } => match reason {
            RecordingReason::Manual | RecordingReason::Api => repo::open(db, camera_id, EventType::Manual, *at, "Manual recording", Some(recording_id)).await,
            // Event, continuous and scheduled clips aren't events themselves:
            // detections happening while they run point at them.
            _ => {
                links.0.insert(camera_id.clone(), recording_id.clone());
                repo::link_open_detections(db, camera_id, recording_id).await
            }
        },
        BusEvent::DetectionStarted { camera_id, kind, topic, at } => {
            let recording = links.0.get(camera_id).map(String::as_str);
            // Watchgrid's own detections are named as such, not as ONVIF topics.
            let (source, origin) = if topic == crate::motion::TOPIC { ("Software motion".to_string(), "software") } else { (format!("ONVIF: {topic}"), "onvif") };
            repo::open_detection(db, camera_id, *kind, *at, &source, recording, origin).await
        }
        BusEvent::SecurityAlert { camera_id, topic, at } => repo::instant(db, camera_id, EventType::Security, *at, &format!("ONVIF: {topic}")).await.map(|()| true),
        BusEvent::DetectionEnded { camera_id, kind, at } => {
            let min = crate::cameras::repo_get(db, camera_id).await.ok().flatten().map_or(0, |c| c.recording.min_event_seconds);
            repo::close_detection(db, camera_id, *kind, *at, min).await
        }
        BusEvent::RecordingStopped { camera_id, recording_id, error, at } => {
            // Detections inside the saved clip that have no clip yet (e.g. the
            // continuous clip began before a restart, so it wasn't tracked).
            if let Some(id) = recording_id {
                repo::link_detections_within(db, id).await?;
            }
            if let Some(started) = links.0.remove(camera_id) {
                if recording_id.is_none() {
                    repo::unlink_recording(db, &started).await?;
                }
                return Ok(true);
            }
            repo::close_recording(db, camera_id, recording_id.as_deref(), *at, error.as_deref()).await
        }
        _ => Ok(false),
    }
}
