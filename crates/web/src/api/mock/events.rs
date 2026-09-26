//! Mock event queries and event actions.

use chrono::{Local, Timelike};

use super::db::with_db;
use super::sim::latency;
use crate::api::{ApiError, ApiResult, Event, EventDetail, EventPage, EventQuery};

fn in_hours(e: &Event, (start, end): (u8, u8)) -> bool {
    // The browser's zone stands in for the server's configured zone.
    let h = e.start_time.with_timezone(&Local).hour() as u8;
    if start <= end { h >= start && h < end } else { h >= start || h < end }
}

fn matches(e: &Event, q: &EventQuery) -> bool {
    q.camera_id.as_ref().is_none_or(|c| &e.camera_id == c)
        && (q.kinds.is_empty() || q.kinds.contains(&e.kind))
        && q.from.is_none_or(|t| e.start_time >= t)
        && q.to.is_none_or(|t| e.start_time < t)
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
