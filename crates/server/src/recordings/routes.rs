//! `/api/v1/recordings` endpoints.

use axum::extract::{Path, Query, Request, State};
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use watchgrid_model::Event;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use watchgrid_model::{RECORDING_BULK_MAX, RECORDING_MATCHING_MAX, Recording, RecordingBulkRequest, RecordingBulkSummary};

use super::repo;
use crate::bus::BusEvent;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/bulk/preview", post(bulk_preview))
        .route("/bulk", post(bulk_apply))
        .route("/{id}", get(one).delete(remove)).route("/{id}/media", get(media)).route("/{id}/protected", put(protect))
}

#[derive(Deserialize)]
struct ListQuery {
    /// Comma-separated camera ids; absent = all cameras.
    cameras: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<Vec<Recording>>> {
    let cameras: Vec<String> = q.cameras.iter().flat_map(|c| c.split(',')).filter(|c| !c.is_empty()).map(String::from).collect();
    let mut list = repo::list(&s.db, &cameras, q.from, q.to).await?;
    // Clips still being written, as far as they got.
    for mut r in s.recorder.live().all().iter().filter_map(|c| c.recording()) {
        let now = Utc::now();
        let wanted = (cameras.is_empty() || cameras.contains(&r.camera_id)) && q.to.is_none_or(|to| r.start_time < to) && q.from.is_none_or(|from| now > from);
        if wanted {
            let events = crate::events::events_of_recording(&s.db, &r.id).await.unwrap_or_default();
            with_events(&mut r, events);
            list.push(r);
        }
    }
    list.sort_by_key(|r| r.start_time);
    Ok(Json(list))
}

async fn one(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Recording>> {
    if let Some(r) = repo::get(&s.db, &id).await? {
        return Ok(Json(r));
    }
    live(&s, &id).await.map(Json).ok_or_else(|| ApiError::not_found("Recording"))
}

/// A recording still being written, with its events.
pub async fn live(s: &AppState, id: &str) -> Option<Recording> {
    let mut r = s.recorder.live().get(id)?.recording()?;
    with_events(&mut r, crate::events::events_of_recording(&s.db, id).await.unwrap_or_default());
    Some(r)
}

/// A clip in progress gets its events (and their protection) from the journal.
fn with_events(r: &mut Recording, events: Vec<Event>) {
    r.protected_by_events = events.iter().filter(|e| e.protected).count() as u32;
    r.event_ids = events.into_iter().map(|e| e.id).collect();
}

/// The MP4 file, with HTTP range support so the browser can seek. A clip
/// still recording is served as far as it got.
async fn media(State(s): State<AppState>, Path(id): Path<String>, req: Request) -> ApiResult<Response> {
    let Some((root, relative)) = repo::path(&s.db, &id).await? else {
        let clip = s.recorder.live().get(&id).ok_or_else(|| ApiError::not_found("Recording"))?;
        let snapshot = clip.snapshot().ok_or_else(|| ApiError::conflict("The recording has no video yet"))?;
        return Ok(super::live_media::serve(snapshot, req.headers()).await);
    };
    let file = s.recording_files.resolve(root.as_deref(), &relative).ok_or_else(|| ApiError::internal(format!("unsafe recording path for {id}")))?;
    match ServeFile::new(file).oneshot(req).await {
        Ok(resp) => Ok(resp.into_response()),
        Err(e) => Err(ApiError::internal(e)),
    }
}

#[derive(Deserialize)]
struct Protect {
    protected: bool,
}

/// Deletes the video; its events stay in the history without it. Refused
/// while recording or protected (by hand or by any of its events).
async fn remove(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if s.recorder.live().get(&id).is_some() {
        return Err(ApiError::conflict("This clip is still being recorded. Stop the recording first."));
    }
    // Its events go too: an event without its video says nothing more.
    let events: Vec<String> = crate::events::events_of_recording(&s.db, &id).await?.into_iter().map(|e| e.id).collect();
    match super::delete_recording(&s.db, &s.recording_files, &id).await {
        Ok(true) => {}
        Ok(false) => return Err(ApiError::not_found("Recording")),
        Err(super::DeleteError::Protected) => {
            return Err(ApiError::conflict("This recording is protected, by hand or by one of its events. Remove the protection first."));
        }
        Err(super::DeleteError::Exporting) => {
            return Err(ApiError::conflict("An upload of this recording is still queued or running. Delete it once the upload is done, or cancel the upload in Settings → Exports → Pending uploads."));
        }
        Err(super::DeleteError::Failed(e)) => return Err(ApiError::internal(e)),
    }
    let removed = crate::events::delete_events(&s.db, &events).await?.len();
    tracing::info!(recording = %id, events = removed, "recording deleted");
    s.bus.publish(BusEvent::RecordingsChanged);
    s.bus.publish(BusEvent::EventsChanged);
    Ok(StatusCode::NO_CONTENT)
}

/// Protected recordings are never deleted by retention. This is the manual
/// protection; protected events keep protecting the clip regardless.
async fn protect(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<Protect>) -> ApiResult<StatusCode> {
    repo::get(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Recording"))?;
    repo::set_protected(&s.db, &id, body.protected).await?;
    s.bus.publish(BusEvent::RecordingsChanged);
    if body.protected {
        s.exports.on_recording_protected(&id, None).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// The recordings a bulk request is about: its ids, or every saved
/// recording matching its query (worked out now).
async fn bulk_ids(s: &AppState, req: &RecordingBulkRequest) -> ApiResult<Vec<String>> {
    let Some(q) = &req.matching else {
        if req.ids.is_empty() || req.ids.len() > RECORDING_BULK_MAX {
            return Err(ApiError::invalid(format!("Select between 1 and {RECORDING_BULK_MAX} recordings")));
        }
        return Ok(req.ids.clone());
    };
    let ids: Vec<String> = repo::list(&s.db, &q.camera_ids, q.from, q.to).await?.into_iter().map(|r| r.id).collect();
    if ids.len() > RECORDING_MATCHING_MAX {
        return Err(ApiError::invalid(format!("More than {RECORDING_MATCHING_MAX} recordings match: narrow the selection first")));
    }
    if ids.is_empty() {
        return Err(ApiError::invalid("No recordings match any more"));
    }
    Ok(ids)
}

/// What a bulk action would do; changes nothing.
async fn bulk_preview(State(s): State<AppState>, Json(req): Json<RecordingBulkRequest>) -> ApiResult<Json<RecordingBulkSummary>> {
    let ids = bulk_ids(&s, &req).await?;
    Ok(Json(super::bulk::preview(&s, req.action, &ids).await?))
}

/// Protect, unprotect or delete many recordings; the summary says what happened.
async fn bulk_apply(State(s): State<AppState>, Json(req): Json<RecordingBulkRequest>) -> ApiResult<Json<RecordingBulkSummary>> {
    let ids = bulk_ids(&s, &req).await?;
    let done = super::bulk::apply(&s, req.action, &ids).await?;
    tracing::info!(action = ?req.action, recordings = done.recordings, events = done.events, "bulk recording action");
    s.bus.publish(BusEvent::RecordingsChanged);
    s.bus.publish(BusEvent::EventsChanged);
    Ok(Json(done))
}
