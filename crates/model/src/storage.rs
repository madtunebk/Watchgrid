//! Storage volume status and retention policy.

use serde::{Deserialize, Serialize};

use crate::{Id, Timestamp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraStorageUsage {
    pub camera_id: Id,
    pub bytes: u64,
    pub recordings: u32,
    pub oldest: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPolicy {
    pub max_age_days: Option<u32>,
    /// Bytes
    pub max_usage: Option<u64>,
    /// Bytes
    pub min_free: Option<u64>,
    /// How long events stay in the history; `None`: as long as recordings
    /// ([`Self::event_days`]).
    #[serde(default)]
    pub event_history_days: Option<u32>,
}

/// Event history without any age rule: a year.
pub const DEFAULT_EVENT_DAYS: u32 = 365;

impl RetentionPolicy {
    /// Days events stay in the history: their own rule, else the recordings'
    /// age limit, else a year.
    pub fn event_days(&self) -> u32 {
        self.event_history_days.or(self.max_age_days).unwrap_or(DEFAULT_EVENT_DAYS)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatus {
    pub path: String,
    /// The volume answers (its size can be read).
    pub available: bool,
    /// New recordings can be written in this folder.
    #[serde(default = "yes")]
    pub writable: bool,
    pub total: u64,
    pub used: u64,
    pub free: u64,
    /// Bytes taken by NVR recordings, in every folder ever used.
    pub recordings_size: u64,
    /// Of those, the bytes in the current folder (what shares this volume).
    #[serde(default)]
    pub recordings_here: u64,
    /// Bytes in protected recordings (never auto-deleted).
    pub protected_size: u64,
    pub per_camera: Vec<CameraStorageUsage>,
    pub retention: RetentionPolicy,
}

fn yes() -> bool {
    true
}

/// What a retention policy would delete right now (before saving it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionPreview {
    pub recordings: u32,
    pub bytes: u64,
    /// Events removed from the history (by the age limit).
    pub events: u64,
}
