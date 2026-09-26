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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatus {
    pub path: String,
    pub available: bool,
    pub total: u64,
    pub used: u64,
    pub free: u64,
    /// Bytes taken by NVR recordings.
    pub recordings_size: u64,
    /// Bytes in protected recordings (never auto-deleted).
    pub protected_size: u64,
    pub per_camera: Vec<CameraStorageUsage>,
    pub retention: RetentionPolicy,
}
