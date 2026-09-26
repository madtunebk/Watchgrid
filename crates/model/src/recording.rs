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
    pub protected: bool,
    pub event_ids: Vec<Id>,
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
