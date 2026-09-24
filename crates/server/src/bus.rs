//! In-process event bus (Tokio broadcast). Carries meaningful transitions,
//! never per-frame observations. Subscribers: the WebSocket hub (UI
//! refresh) and the event journal (durable events); rules and
//! notifications later.

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use watchgrid_model::{EventType, RecordingReason};

#[derive(Debug, Clone, PartialEq)]
pub enum BusEvent {
    CameraOnline { camera_id: String, at: DateTime<Utc> },
    CameraOffline { camera_id: String, reason: String, at: DateTime<Utc> },
    /// Supervision ended because the camera was disabled or deleted.
    CameraStopped { camera_id: String, at: DateTime<Utc> },
    /// Camera configuration changed (added, edited, removed, enabled…).
    CamerasChanged,
    /// The camera reported a detection starting (ONVIF event).
    DetectionStarted { camera_id: String, kind: EventType, topic: String, at: DateTime<Utc> },
    /// …and ending.
    DetectionEnded { camera_id: String, kind: EventType, at: DateTime<Utc> },
    /// Storage settings changed (e.g. the retention policy).
    StorageChanged,
    /// Recordings were removed or changed (retention, protection, deletion).
    RecordingsChanged,
    /// A recording wrote its first frame.
    RecordingStarted { camera_id: String, recording_id: String, reason: RecordingReason, at: DateTime<Utc> },
    /// A recording ended; `recording_id` is set when a file was saved.
    RecordingStopped { camera_id: String, recording_id: Option<String>, error: Option<String>, at: DateTime<Utc> },
    /// The Settings page document changed.
    SettingsChanged,
    /// The event journal stored or changed events.
    EventsChanged,
}

#[derive(Clone)]
pub struct Bus(broadcast::Sender<BusEvent>);

impl Bus {
    pub fn new() -> Self {
        Self(broadcast::channel(256).0)
    }

    /// Fire and forget; having no subscribers is fine.
    pub fn publish(&self, event: BusEvent) {
        let _ = self.0.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.0.subscribe()
    }
}
