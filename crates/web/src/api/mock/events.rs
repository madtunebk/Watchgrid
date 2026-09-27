//! Mock event queries and event actions.

use chrono::{DateTime, Local, Timelike, Utc};

use super::db::with_db;
use super::sim::latency;
use std::collections::HashSet;

use crate::api::{ApiError, ApiResult, BulkSkip, BulkSkipReason, Event, EventBulkAction, EventBulkRequest, EventBulkSummary, EventDetail, EventPage, EventQuery, NvrDays};

fn in_hours(e: &Event, (start, end): (u8, u8)) -> bool {
    // The browser's zone stands in for the server's configured zone.
    let h = e.start_time.with_timezone(&Local).hour() as u8;
    if start <= end { h >= start && h < end } else { h >= start || h < end }
}

/// The demo has no NVR zone: the browser's days stand in for it.
fn local_days(days: NvrDays) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = crate::clock::start_of_today();
    match days {
        NvrDays::Today => (today, today + chrono::Duration::days(1)),
        NvrDays::Yesterday => (today - chrono::Duration::days(1), today),
        NvrDays::Week => (today - chrono::Duration::days(6), today + chrono::Duration::days(1)),
        NvrDays::Date(d) => {
            let start = d.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map_or(today, |t| t.to_utc());
            (start, start + chrono::Duration::days(1))
        }
    }
}

fn matches(e: &Event, q: &EventQuery) -> bool {
    let (from, to) = match q.days.map(local_days) {
        Some((f, t)) => (Some(f), Some(t)),
        None => (q.from, q.to),
    };
    q.camera_id.as_ref().is_none_or(|c| &e.camera_id == c)
        && (q.kinds.is_empty() || q.kinds.contains(&e.kind))
        && from.is_none_or(|t| e.start_time >= t)
        && to.is_none_or(|t| e.start_time < t)
        && q.hours.is_none_or(|h| in_hours(e, h))
        && q.min_duration.is_none_or(|d| e.duration >= d)
        && (!q.protected_only || e.protected)
}

pub async fn list(query: &EventQuery) -> ApiResult<EventPage> {
    latency().await;
    Ok(with_db(|db| {
        let all: Vec<&Event> = db.events.iter().filter(|e| matches(e, query)).collect();
        let offset = query.offset.unwrap_or(0) as usize;
        let limit = query.limit.map_or(usize::MAX, |l| l as usize);
        EventPage { total: all.len() as u32, events: all.into_iter().skip(offset).take(limit).cloned().collect() }
    }))
}

pub async fn get(id: &str) -> ApiResult<EventDetail> {
    latency().await;
    with_db(|db| {
        // `db.events` is newest first.
        let i = db.events.iter().position(|e| e.id == id).ok_or_else(|| ApiError::not_found("Event"))?;
        let event = db.events[i].clone();
        let recording = event.recording_id.as_ref().and_then(|r| db.recordings.iter().find(|x| &x.id == r).cloned());
        Ok(EventDetail {
            next: i.checked_sub(1).map(|j| db.events[j].id.clone()),
            previous: db.events.get(i + 1).map(|e| e.id.clone()),
            event,
            recording,
        })
    })
}

pub async fn set_protected(id: &str, protected: bool) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let e = db.events.iter_mut().find(|e| e.id == id).ok_or_else(|| ApiError::not_found("Event"))?;
        e.protected = protected;
        if let Some(rec) = e.recording_id.clone() {
            db.recount_protection(&rec);
        }
        Ok(())
    })
}

pub async fn delete(id: &str) -> ApiResult<()> {
    latency().await;
    with_db(|db| {
        let i = db.events.iter().position(|e| e.id == id).ok_or_else(|| ApiError::not_found("Event"))?;
        if db.events[i].protected {
            return Err(ApiError::conflict("This event is protected. Remove protection before deleting it."));
        }
        // The recording stays (as on the server); it only loses this event.
        let event = db.events.remove(i);
        if let Some(r) = db.recordings.iter_mut().find(|r| Some(&r.id) == event.recording_id.as_ref()) {
            r.event_ids.retain(|e| e != &event.id);
        }
        Ok(())
    })
}

/// Same rules as the server's `events/bulk.rs`: returns the summary and
/// the recordings to delete.
fn plan(db: &super::db::Db, req: &EventBulkRequest) -> (EventBulkSummary, Vec<String>, Vec<String>) {
    let mut sum = EventBulkSummary::default();
    let (mut events, mut seen) = (Vec::new(), HashSet::new());
    let skip = |id: &str, reason| BulkSkip { id: id.to_string(), reason };
    // "All matching": the filter's events instead of the ticked ones.
    let wanted: Vec<String> = match &req.matching {
        Some(q) => db.events.iter().filter(|e| matches(e, q)).map(|e| e.id.clone()).collect(),
        None => req.ids.clone(),
    };
    for id in wanted.iter().filter(|id| seen.insert(id.as_str())) {
        let Some(e) = db.events.iter().find(|e| &e.id == id) else {
            sum.skipped_events.push(skip(id, BulkSkipReason::NotFound));
            continue;
        };
        match req.action {
            EventBulkAction::Protect if !e.protected => events.push(id.clone()),
            EventBulkAction::Unprotect if e.protected => events.push(id.clone()),
            EventBulkAction::Protect | EventBulkAction::Unprotect => {}
            _ if e.protected => sum.skipped_events.push(skip(id, BulkSkipReason::Protected)),
            _ if e.end_time.is_none() && e.recording_id.is_some() => sum.skipped_events.push(skip(id, BulkSkipReason::InProgress)),
            _ => events.push(id.clone()),
        }
    }
    let mut recordings = Vec::new();
    if req.action == EventBulkAction::DeleteWithVideo {
        let gone: HashSet<&str> = events.iter().map(String::as_str).collect();
        let mut wanted: Vec<&str> = db.events.iter().filter(|e| gone.contains(e.id.as_str())).filter_map(|e| e.recording_id.as_deref()).collect();
        wanted.sort_unstable();
        wanted.dedup();
        for rid in wanted {
            let Some(r) = db.recordings.iter().find(|r| r.id == rid) else { continue };
            let shared = db.events.iter().any(|e| e.recording_id.as_deref() == Some(rid) && !gone.contains(e.id.as_str()));
            if r.end_time.is_none() {
                sum.kept_recordings.push(skip(rid, BulkSkipReason::Recording));
            } else if r.is_protected() {
                sum.kept_recordings.push(skip(rid, BulkSkipReason::Protected));
            } else if shared {
                sum.kept_recordings.push(skip(rid, BulkSkipReason::Shared));
            } else {
                sum.recordings += 1;
                sum.bytes += r.file_size;
                recordings.push(rid.to_string());
            }
        }
    }
    sum.events = events.len() as u32;
    (sum, events, recordings)
}

pub async fn bulk_preview(req: &EventBulkRequest) -> ApiResult<EventBulkSummary> {
    latency().await;
    Ok(with_db(|db| plan(db, req).0))
}

pub async fn bulk_apply(req: &EventBulkRequest) -> ApiResult<EventBulkSummary> {
    latency().await;
    Ok(with_db(|db| {
        let (sum, events, recordings) = plan(db, req);
        match req.action {
            EventBulkAction::Protect | EventBulkAction::Unprotect => {
                let protect = req.action == EventBulkAction::Protect;
                let mut clips = Vec::new();
                for e in db.events.iter_mut().filter(|e| events.contains(&e.id)) {
                    e.protected = protect;
                    clips.extend(e.recording_id.clone());
                }
                for c in clips {
                    db.recount_protection(&c);
                }
            }
            EventBulkAction::Delete | EventBulkAction::DeleteWithVideo => {
                db.recordings.retain(|r| !recordings.contains(&r.id));
                db.events.retain(|e| !events.contains(&e.id));
                for r in db.recordings.iter_mut() {
                    r.event_ids.retain(|e| !events.contains(e));
                }
            }
        }
        sum
    }))
}
