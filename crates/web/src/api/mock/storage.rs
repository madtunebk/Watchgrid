//! Mock storage status, derived from the recordings in the mock database.

use std::collections::BTreeMap;

use super::db::with_db;
use super::scenario::Scenario;
use super::sim::latency;
use crate::api::{ApiResult, CameraStorageUsage, RetentionPolicy, StorageStatus};

const TB: u64 = 1_000_000_000_000;
/// Non-NVR data on the same volume (other NAS shares).
const OTHER_DATA: u64 = 1_420_000_000_000;

pub async fn status() -> ApiResult<StorageStatus> {
    latency().await;
    Ok(with_db(|db| {
        let mut per_camera: BTreeMap<&str, CameraStorageUsage> = BTreeMap::new();
        for r in &db.recordings {
            let u = per_camera.entry(&r.camera_id).or_insert_with(|| CameraStorageUsage {
                camera_id: r.camera_id.clone(),
                bytes: 0,
                recordings: 0,
                oldest: None,
            });
            u.bytes += r.file_size;
            u.recordings += 1;
            u.oldest = Some(u.oldest.map_or(r.start_time, |o| o.min(r.start_time)));
        }
        let mut per_camera: Vec<_> = per_camera.into_values().collect();
        per_camera.sort_by(|a, b| b.bytes.cmp(&a.bytes));

        let recordings_size: u64 = per_camera.iter().map(|u| u.bytes).sum();
        let protected_size = db.recordings.iter().filter(|r| r.protected).map(|r| r.file_size).sum();
        let available = db.scenario != Scenario::NoStorage;
        let total = if available { 4 * TB } else { 0 };
        let used = if available { OTHER_DATA + recordings_size } else { 0 };

        StorageStatus {
            path: "/volume1/nvr".into(),
            available,
            total,
            used,
            free: total.saturating_sub(used),
            recordings_size,
            protected_size,
            per_camera,
            retention: db.retention.clone(),
        }
    }))
}

pub async fn update_retention(policy: RetentionPolicy) -> ApiResult<()> {
    latency().await;
    if policy.max_age_days == Some(0) {
        return Err(crate::api::ApiError::new(422, "invalid", "Keep recordings for at least 1 day"));
    }
    with_db(|db| db.retention = policy);
    Ok(())
}
