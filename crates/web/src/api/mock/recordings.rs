//! Mock recording queries.

use chrono::Utc;

use super::db::with_db;
use super::sim::latency;
use crate::api::{ApiError, ApiResult, Recording, RecordingQuery};

fn matches(r: &Recording, q: &RecordingQuery) -> bool {
    let end = r.end_time.unwrap_or_else(Utc::now);
    (q.camera_ids.is_empty() || q.camera_ids.contains(&r.camera_id))
        && q.to.is_none_or(|to| r.start_time < to)
        && q.from.is_none_or(|from| end > from)
}

/// Mock clips have no video file.
pub fn media_url(_id: &str) -> Option<String> {
    None
}

pub async fn list(query: &RecordingQuery) -> ApiResult<Vec<Recording>> {
    latency().await;
    Ok(with_db(|db| {
        let mut list: Vec<Recording> = db.recordings.iter().filter(|r| matches(r, query)).cloned().collect();
        list.sort_by_key(|r| r.start_time);
        list
    }))
}

pub async fn set_protected(id: &str, protected: bool) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let r = db.recordings.iter_mut().find(|r| r.id == id).ok_or_else(|| crate::api::ApiError::not_found("Recording"))?;
        r.protected = protected;
        Ok(())
    })
}

pub async fn delete(id: &str) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let r = db.recordings.iter().find(|r| r.id == id).ok_or_else(|| ApiError::not_found("Recording"))?;
        if r.end_time.is_none() {
            return Err(ApiError::conflict("This clip is still being recorded. Stop the recording first."));
        }
        if r.is_protected() {
            return Err(ApiError::conflict("This recording is protected, by hand or by one of its events. Remove the protection first."));
        }
        db.recordings.retain(|r| r.id != id);
        for e in db.events.iter_mut().filter(|e| e.recording_id.as_deref() == Some(id)) {
            e.recording_id = None;
        }
        Ok(())
    })
}
