//! `/api/v1/recordings` endpoints.

use axum::extract::{Path, Query, Request, State};
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use watchgrid_model::Recording;

use super::repo;
use crate::bus::BusEvent;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list)).route("/{id}", get(one)).route("/{id}/media", get(media)).route("/{id}/protected", put(protect))
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
            r.event_ids = crate::events::events_of_recording(&s.db, &r.id).await.unwrap_or_default().into_iter().map(|e| e.id).collect();
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
    r.event_ids = crate::events::events_of_recording(&s.db, id).await.unwrap_or_default().into_iter().map(|e| e.id).collect();
    Some(r)
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

/// Protected recordings are never deleted by retention.
async fn protect(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<Protect>) -> ApiResult<StatusCode> {
    repo::get(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Recording"))?;
    repo::set_protected(&s.db, &id, body.protected).await?;
    s.bus.publish(BusEvent::RecordingsChanged);
    Ok(StatusCode::NO_CONTENT)
}
