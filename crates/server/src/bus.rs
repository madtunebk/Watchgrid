//! In-process event bus (Tokio broadcast). Carries meaningful transitions,
//! never per-frame observations. Subscribers: the WebSocket hub now; the
//! recorder, rules, notifications and DB persistence later.

use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq)]
pub enum BusEvent {
    CameraOnline { camera_id: String },
    CameraOffline { camera_id: String, reason: String },
    /// Camera configuration changed (added, edited, removed, enabled…).
    CamerasChanged,
    /// Storage settings changed (e.g. the retention policy).
    StorageChanged,
    /// Retention removed old recordings.
    RecordingsDeleted,
    /// A recording wrote its first frame.
    RecordingStarted { camera_id: String },
    /// A recording ended; `recording_id` is set when a file was saved.
    RecordingStopped { camera_id: String, recording_id: Option<String>, error: Option<String> },
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
