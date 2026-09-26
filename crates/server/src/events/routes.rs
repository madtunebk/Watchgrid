//! `/api/v1/events` endpoints.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use watchgrid_model::{EventDetail, EventPage, EventQuery};

use super::{kinds, repo};
use crate::bus::BusEvent;
use crate::error::{ApiError, ApiResult};
use crate::recordings;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list)).route("/{id}", get(one).delete(remove)).route("/{id}/protected", put(protect))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    camera: Option<String>,
    /// Comma-separated event types.
    kinds: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    /// `start-end` local hours, e.g. `22-6`.
    hours: Option<String>,
    min_duration: Option<u32>,
    #[serde(default)]
    protected_only: bool,
    limit: Option<u32>,
    offset: Option<u32>,
}

impl ListQuery {
    fn into_query(self) -> ApiResult<EventQuery> {
        let kinds = match self.kinds.as_deref().filter(|k| !k.is_empty()) {
            None => Vec::new(),
            Some(list) => list.split(',').map(|k| kinds::parse(k).ok_or_else(|| ApiError::invalid(format!("unknown event type `{k}`")))).collect::<ApiResult<_>>()?,
        };
        let hours = match self.hours.as_deref() {
            None => None,
            Some(h) => Some(parse_hours(h).ok_or_else(|| ApiError::invalid("hours must look like 22-6 (0–24)"))?),
        };
        Ok(EventQuery {
            camera_id: self.camera.filter(|c| !c.is_empty()),
            kinds,
            from: self.from,
            to: self.to,
            hours,
            min_duration: self.min_duration,
            protected_only: self.protected_only,
            limit: self.limit,
            offset: self.offset,
        })
    }
}

fn parse_hours(s: &str) -> Option<(u8, u8)> {
    let (a, b) = s.split_once('-')?;
    let (a, b): (u8, u8) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (a <= 24 && b <= 24).then_some((a, b))
}

async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<EventPage>> {
    let tz = crate::settings::load_app(&s.db, s.bind).await?.general.timezone;
    let (events, total) = repo::list(&s.db, &q.into_query()?, &tz).await?;
    Ok(Json(EventPage { events, total }))
}

async fn one(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<EventDetail>> {
    let event = repo::get(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Event"))?;
    let recording = match &event.recording_id {
        // Saved, or still being written.
        Some(r) => match recordings::get(&s.db, r).await? {
            Some(saved) => Some(saved),
            None => recordings::live_recording(&s, r).await,
        },
        None => None,
    };
    let (previous, next) = repo::neighbours(&s.db, &event).await?;
    Ok(Json(EventDetail { event, recording, previous, next }))
}

#[derive(Deserialize)]
struct Protect {
    protected: bool,
}

/// Protecting an event also protects its recording from retention.
async fn protect(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<Protect>) -> ApiResult<StatusCode> {
    let event = repo::get(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Event"))?;
    repo::set_protected(&s.db, &id, body.protected).await?;
    if let Some(r) = &event.recording_id {
        recordings::set_protected(&s.db, r, body.protected).await?;
    }
    s.bus.publish(BusEvent::EventsChanged);
    s.bus.publish(BusEvent::RecordingsChanged);
    if body.protected {
        s.exports.on_protected(&id).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes the event and its recording. Refused while protected.
async fn remove(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let event = repo::get(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Event"))?;
    if event.protected {
        return Err(ApiError::conflict("This event is protected. Remove protection before deleting it."));
    }
    if event.end_time.is_none() && event.recording_id.is_some() {
        return Err(ApiError::conflict("This recording is still running. Stop it before deleting the event."));
    }
    if let Some(r) = &event.recording_id {
        recordings::delete_recording(&s.db, &s.recording_files, r).await.map_err(ApiError::internal)?;
        s.bus.publish(BusEvent::RecordingsChanged);
    }
    repo::delete(&s.db, &id).await?;
    tracing::info!(event = %id, "event deleted");
    s.bus.publish(BusEvent::EventsChanged);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::parse_hours;

    #[test]
    fn hours_parse_and_validate() {
        assert_eq!(parse_hours("22-6"), Some((22, 6)));
        assert_eq!(parse_hours("8-18"), Some((8, 18)));
        assert_eq!(parse_hours("25-3"), None);
        assert_eq!(parse_hours("8"), None);
    }
}
