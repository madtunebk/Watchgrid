//! Changing many recordings at once (Recordings → Clips → select → action).

use serde::{Deserialize, Serialize};

use crate::{BulkSkip, Id, RecordingQuery};

/// Most recordings one request may change when they are listed by id.
pub const RECORDING_BULK_MAX: usize = 500;
/// Most recordings "all matching" may change at once.
pub const RECORDING_MATCHING_MAX: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingBulkAction {
    /// Protect by hand.
    Protect,
    /// Remove the manual protection (protected events still protect).
    Unprotect,
    /// Delete the videos; their events stay in the history without video.
    Delete,
    /// Delete the videos and their events.
    DeleteWithEvents,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingBulkRequest {
    pub ids: Vec<Id>,
    pub action: RecordingBulkAction,
    /// Instead of `ids`: every saved recording matching this query.
    #[serde(default)]
    pub matching: Option<RecordingQuery>,
}

/// What a bulk action will do (preview) or did (result).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingBulkSummary {
    /// Recordings changed (or deleted).
    pub recordings: u32,
    /// Bytes of the deleted recordings.
    pub bytes: u64,
    /// Events deleted with them (`DeleteWithEvents` only).
    pub events: u32,
    /// Left alone, or (after Unprotect) still protected by their events.
    pub skipped: Vec<BulkSkip>,
}
