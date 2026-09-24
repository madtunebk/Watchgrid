//! The event journal: turns bus transitions into durable events.
//!
//! camera offline  → opens an outage event (idempotent while it lasts)
//! camera online   → closes the outage and records a "back online" event
//! camera stopped  → closes the outage (disabled or deleted)
//! recording start → opens a recording event linked to the recording
//! recording stop  → closes it with the recording's real end time

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
        loop {
            match events.recv().await {
                Ok(event) => match handle(&db, &event).await {
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

/// Store one transition. Returns whether anything changed.
pub async fn handle(db: &PgPool, event: &BusEvent) -> sqlx::Result<bool> {
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
        BusEvent::RecordingStarted { camera_id, recording_id, reason, at } => {
            let (kind, source) = match reason {
                RecordingReason::Scheduled => (EventType::Scheduled, "Scheduled recording"),
                _ => (EventType::Manual, "Manual recording"),
            };
            repo::open(db, camera_id, kind, *at, source, Some(recording_id)).await
        }
        BusEvent::RecordingStopped { camera_id, recording_id, error, at } => {
            repo::close_recording(db, camera_id, recording_id.as_deref(), *at, error.as_deref()).await
        }
        _ => Ok(false),
    }
}
