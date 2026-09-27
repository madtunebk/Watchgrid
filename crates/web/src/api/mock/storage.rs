//! Mock storage status, derived from the recordings in the mock database.

use std::collections::BTreeMap;

use super::db::with_db;
use super::scenario::Scenario;
use super::sim::latency;
use crate::api::{ApiResult, WriteRate, CameraStorageUsage, RetentionPolicy, StorageStatus};

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
        let protected_size = db.recordings.iter().filter(|r| r.is_protected()).map(|r| r.file_size).sum();
        let available = db.scenario != Scenario::NoStorage;
        let total = if available { 4 * TB } else { 0 };
        let used = if available { OTHER_DATA + recordings_size } else { 0 };

        StorageStatus {
            writable: true,
            recordings_here: recordings_size,
            path: "/volume1/nvr".into(),
            available,
            total,
            used,
            free: total.saturating_sub(used),
            recordings_size,
            protected_size,
            per_camera,
            retention: db.retention.clone(),
            // Same measure as the server: the last week of recordings.
            write_rate: {
                let since = chrono::Utc::now() - chrono::Duration::days(7);
                let first = db.recordings.iter().map(|r| r.start_time).min();
                first.map(|f| f.max(since)).and_then(|from| {
                    let hours = (chrono::Utc::now() - from).num_seconds() as f64 / 3600.0;
                    let bytes: u64 = db.recordings.iter().filter(|r| r.start_time >= from).map(|r| r.file_size).sum();
                    (hours >= 1.0).then(|| WriteRate { bytes_per_day: (bytes as f64 / hours * 24.0) as u64, window_hours: hours.round() as u32 })
                })
            },
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

pub async fn set_path(path: &str) -> ApiResult<()> {
    latency().await;
    if !path.starts_with('/') {
        return Err(crate::api::ApiError::new(422, "invalid", "Use an absolute path, e.g. /volume1/watchgrid"));
    }
    Ok(())
}

/// Age rule only (enough for the demo): unprotected clips older than it.
pub async fn preview_retention(policy: RetentionPolicy) -> ApiResult<crate::api::RetentionPreview> {
    latency().await;
    Ok(with_db(|db| {
        let cutoff = policy.max_age_days.map(|d| chrono::Utc::now() - chrono::Duration::days(i64::from(d)));
        let old: Vec<_> = db.recordings.iter().filter(|r| !r.is_protected() && cutoff.is_some_and(|c| r.end_time.is_some_and(|e| e < c))).collect();
        let events = cutoff.map_or(0, |c| db.events.iter().filter(|e| !e.protected && e.end_time.is_some_and(|t| t < c)).count()) as u64;
        crate::api::RetentionPreview { recordings: old.len() as u32, bytes: old.iter().map(|r| r.file_size).sum(), events }
    }))
}
