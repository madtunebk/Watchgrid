//! Mock recording queries.

use chrono::Utc;

use super::db::with_db;
use super::sim::latency;
use std::collections::HashSet;

use crate::api::{ApiError, ApiResult, BulkSkip, BulkSkipReason, Recording, RecordingBulkAction, RecordingBulkRequest, RecordingBulkSummary, RecordingQuery};

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
        db.events.retain(|e| e.recording_id.as_deref() != Some(id));
        Ok(())
    })
}

/// Same rules as the server's `recordings/bulk.rs`; also returns the ids to change.
fn plan(db: &super::db::Db, req: &RecordingBulkRequest) -> (RecordingBulkSummary, Vec<String>) {
    let mut sum = RecordingBulkSummary::default();
    let (mut ids, mut seen) = (Vec::new(), HashSet::new());
    let skip = |id: &str, reason| BulkSkip { id: id.to_string(), reason };
    for id in req.ids.iter().filter(|id| seen.insert(id.as_str())) {
        let Some(r) = db.recordings.iter().find(|r| &r.id == id) else {
            sum.skipped.push(skip(id, BulkSkipReason::NotFound));
            continue;
        };
        match req.action {
            _ if r.end_time.is_none() => sum.skipped.push(skip(id, BulkSkipReason::Recording)),
            RecordingBulkAction::Protect if !r.protected => ids.push(id.clone()),
            RecordingBulkAction::Protect => {}
            RecordingBulkAction::Unprotect => {
                if r.protected {
                    ids.push(id.clone());
                }
                if r.protected_by_events > 0 {
                    sum.skipped.push(skip(id, BulkSkipReason::Protected));
                }
            }
            _ if r.is_protected() => sum.skipped.push(skip(id, BulkSkipReason::Protected)),
            _ => {
                ids.push(id.clone());
                sum.bytes += r.file_size;
                if req.action == RecordingBulkAction::DeleteWithEvents {
                    sum.events += r.event_ids.len() as u32;
                }
            }
        }
    }
    sum.recordings = ids.len() as u32;
    (sum, ids)
}

pub async fn bulk_preview(req: &RecordingBulkRequest) -> ApiResult<RecordingBulkSummary> {
    latency().await;
    Ok(with_db(|db| plan(db, req).0))
}

pub async fn bulk_apply(req: &RecordingBulkRequest) -> ApiResult<RecordingBulkSummary> {
    latency().await;
    Ok(with_db(|db| {
        let (sum, ids) = plan(db, req);
        match req.action {
            RecordingBulkAction::Protect | RecordingBulkAction::Unprotect => {
                for r in db.recordings.iter_mut().filter(|r| ids.contains(&r.id)) {
                    r.protected = req.action == RecordingBulkAction::Protect;
                }
            }
            RecordingBulkAction::Delete | RecordingBulkAction::DeleteWithEvents => {
                db.recordings.retain(|r| !ids.contains(&r.id));
                if req.action == RecordingBulkAction::DeleteWithEvents {
                    db.events.retain(|e| !e.recording_id.as_ref().is_some_and(|r| ids.contains(r)));
                } else {
                    for e in db.events.iter_mut().filter(|e| e.recording_id.as_ref().is_some_and(|r| ids.contains(r))) {
                        e.recording_id = None;
                    }
                }
            }
        }
        sum
    }))
}
