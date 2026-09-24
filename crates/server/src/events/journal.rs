//! The event journal: turns bus transitions into durable events.
//!
//! camera offline  → opens an outage event (idempotent while it lasts)
//! camera online   → closes the outage and records a "back online" event
//! camera stopped  → closes the outage (disabled or deleted)
//! recording start → opens a recording event linked to the recording
//! recording stop  → closes it with the recording's real end time
//! ONVIF detection → opens/closes a motion/person/… event (origin "onvif")

use std::collections::HashMap;

use sqlx::PgPool;
use tokio::sync::broadcast::error::RecvError;
use watchgrid_model::{EventType, RecordingReason};

use super::repo;
use crate::bus::{Bus, BusEvent};

/// Subscribe now (so nothing published after this call is missed) and
/// process events in the background.
pub fn start(db: PgPool, bus: Bus) {
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
                Err(RecvError::Lagged(n)) => tracing::warn!("event journal missed {n} bus messages"),
                Err(RecvError::Closed) => return,
            }
        }
    });
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
            repo::open_from(db, camera_id, *kind, *at, &format!("ONVIF: {topic}"), recording, "onvif").await
        }
        BusEvent::DetectionEnded { camera_id, kind, at } => repo::close(db, camera_id, *kind, *at).await,
        BusEvent::RecordingStopped { camera_id, recording_id, error, at } => {
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
