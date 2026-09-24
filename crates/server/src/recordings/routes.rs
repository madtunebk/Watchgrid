//! `/api/v1/recordings` endpoints.

use axum::extract::{Path, Query, Request, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use watchgrid_model::Recording;

use super::repo;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list)).route("/{id}", get(one)).route("/{id}/media", get(media))
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
    Ok(Json(repo::list(&s.db, &cameras, q.from, q.to).await?))
}

async fn one(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Recording>> {
    repo::get(&s.db, &id).await?.map(Json).ok_or_else(|| ApiError::not_found("Recording"))
}

/// The MP4 file, with HTTP range support so the browser can seek.
async fn media(State(s): State<AppState>, Path(id): Path<String>, req: Request) -> ApiResult<Response> {
    let relative = repo::path(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Recording"))?;
    let file = s.recording_files.resolve(&relative).ok_or_else(|| ApiError::internal(format!("unsafe recording path for {id}")))?;
    match ServeFile::new(file).oneshot(req).await {
        Ok(resp) => Ok(resp.into_response()),
        Err(e) => Err(ApiError::internal(e)),
    }
}
