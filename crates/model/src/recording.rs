//! Recordings: clips written to disk.

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingReason {
    Motion,
    Event,
    Manual,
    Continuous,
    Scheduled,
    Api,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub id: Id,
    pub camera_id: Id,
    pub start_time: Timestamp,
    pub end_time: Option<Timestamp>,
    pub duration: u32,
    pub reason: RecordingReason,
    /// Bytes
    pub file_size: u64,
    /// Protected by hand (Recordings → Protect).
    pub protected: bool,
    /// Protected events in this clip; each one also keeps it from deletion.
    #[serde(default)]
    pub protected_by_events: u32,
    pub event_ids: Vec<Id>,
}

impl Recording {
    /// Kept from retention and deletion, by hand or by a protected event.
    pub fn is_protected(&self) -> bool {
        self.protected || self.protected_by_events > 0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingQuery {
    /// Empty = all cameras.
    pub camera_ids: Vec<Id>,
    /// Recordings overlapping `[from, to)`.
    pub from: Option<Timestamp>,
    pub to: Option<Timestamp>,
}
