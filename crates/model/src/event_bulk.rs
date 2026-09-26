//! Changing many events at once (Events → select → action).

use serde::{Deserialize, Serialize};

use crate::Id;

/// Most events one request may change.
pub const EVENT_BULK_MAX: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventBulkAction {
    Protect,
    Unprotect,
    /// Remove the events from the history; their recordings stay.
    Delete,
    /// Also delete the recordings, when nothing else needs them.
    DeleteWithVideo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBulkRequest {
    pub ids: Vec<Id>,
    pub action: EventBulkAction,
}

/// Why an event or a recording was left alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkSkipReason {
    /// The event, or the recording (by hand or by an event), is protected.
    Protected,
    /// The event is still going on.
    InProgress,
    NotFound,
    /// The recording also holds events that were not selected.
    Shared,
    /// The recording is still being written.
    Recording,
    /// Deleting failed (see the server log).
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkSkip {
    pub id: Id,
    pub reason: BulkSkipReason,
}

/// What a bulk action will do (preview) or did (result).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventBulkSummary {
    /// Events changed (or deleted).
    pub events: u32,
    /// Recordings deleted (`DeleteWithVideo` only).
    pub recordings: u32,
    /// Bytes of those recordings.
    pub bytes: u64,
    pub skipped_events: Vec<BulkSkip>,
    /// Recordings of deleted events that were kept.
    pub kept_recordings: Vec<BulkSkip>,
}
