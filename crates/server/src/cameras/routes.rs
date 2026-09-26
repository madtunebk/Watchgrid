//! `/api/v1/cameras` endpoints.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use watchgrid_model::{Camera, CameraInput, ConnectionProbe, ConnectionTest, OnvifConfig, OnvifProbe, StreamProbe, StreamTest};

use super::{probes, service};
use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/test-connection", post(test_connection))
        .route("/test-stream", post(test_stream))
        .route("/test-onvif", post(test_onvif))
        .route("/{id}", get(one).put(update).delete(remove))
        .route("/{id}/enable", post(enable))
        .route("/{id}/disable", post(disable))
        .route("/{id}/recording/start", post(start_recording))
        .route("/{id}/recording/stop", post(stop_recording))
        .merge(super::ptz::router())
}

async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<Camera>>> {
    service::list(&s).await.map(Json)
}

async fn one(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Camera>> {
    service::get(&s, &id).await.map(Json)
}

async fn create(State(s): State<AppState>, Json(input): Json<CameraInput>) -> ApiResult<(StatusCode, Json<Camera>)> {
    service::create(&s, input).await.map(|c| (StatusCode::CREATED, Json(c)))
}

async fn update(State(s): State<AppState>, Path(id): Path<String>, Json(input): Json<CameraInput>) -> ApiResult<Json<Camera>> {
    service::update(&s, &id, input).await.map(Json)
}

async fn remove(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    service::delete(&s, &id).await.map(|_| StatusCode::NO_CONTENT)
}

async fn enable(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Camera>> {
    service::set_enabled(&s, &id, true).await.map(Json)
}

async fn disable(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Camera>> {
    service::set_enabled(&s, &id, false).await.map(Json)
}

async fn start_recording(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Camera>> {
    service::start_recording(&s, &id).await.map(Json)
}

/// Returns once the recording is finalized and listed.
async fn stop_recording(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Camera>> {
    service::stop_recording(&s, &id).await.map(Json)
}

async fn test_connection(Json(req): Json<ConnectionTest>) -> ApiResult<Json<ConnectionProbe>> {
    probes::connection(req).await.map(Json)
}

async fn test_stream(State(s): State<AppState>, Json(req): Json<StreamTest>) -> ApiResult<Json<StreamProbe>> {
    probes::stream(&s, req).await.map(Json)
}

async fn test_onvif(State(s): State<AppState>, Json(config): Json<OnvifConfig>) -> ApiResult<Json<OnvifProbe>> {
    probes::onvif(&s, config).await.map(Json)
}
