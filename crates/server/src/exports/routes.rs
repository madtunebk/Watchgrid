//! `/api/v1/exports/*`, `POST /api/v1/events/{id}/export` and
//! `POST /api/v1/recordings/{id}/export`.

use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use watchgrid_model::{AutoUpload, ConnectionProbe, ExportJob, ExportTarget, ExportTargetInput, ExportTargetSettings};

use super::providers::{Provider, TargetConfig};
use super::repo::{self, StoredTarget};
use super::service::secret_aad;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/targets", get(list).post(create))
        .route("/targets/test", post(test))
        .route("/targets/{id}", get(settings).put(update).delete(remove))
        .route("/targets/{id}/test", post(test_saved))
        .route("/targets/{id}/auto-upload", put(set_auto))
        .route("/targets/{id}/reconnect", post(reconnect))
        .route("/targets/{id}/check", post(check))
        .route("/jobs", get(active_jobs))
        .route("/jobs/{id}", get(job))
        .route("/jobs/{id}/cancel", post(cancel_job))
}

async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<ExportTarget>>> {
    Ok(Json(repo::targets(&s.db).await?.iter().map(StoredTarget::public).collect()))
}

fn config(i: &ExportTargetInput) -> TargetConfig<'_> {
    TargetConfig { kind: i.kind, endpoint: &i.endpoint, location: &i.location, username: &i.username, secret: i.secret.as_deref().unwrap_or("") }
}

async fn probe(provider: Result<Provider, String>) -> ConnectionProbe {
    let started = Instant::now();
    let result = match provider {
        Ok(p) => p.test().await,
        Err(e) => Err(e),
    };
    let latency_ms = Some(started.elapsed().as_millis() as u32);
    match result {
        Ok(message) => ConnectionProbe { ok: true, message, latency_ms, device: None },
        Err(message) => ConnectionProbe { ok: false, message, latency_ms: None, device: None },
    }
}

/// Try the credentials before saving (nothing is stored).
async fn test(Json(input): Json<ExportTargetInput>) -> Json<ConnectionProbe> {
    Json(probe(Provider::new(&config(&input))).await)
}

/// Save a destination; it must pass the test first.
async fn create(State(s): State<AppState>, Json(input): Json<ExportTargetInput>) -> ApiResult<Json<ExportTarget>> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::invalid("Give the destination a name"));
    }
    let p = probe(Provider::new(&config(&input))).await;
    if !p.ok {
        return Err(ApiError::invalid(p.message));
    }
    let id = repo::new_target_id(&s.db).await?;
    let secret_enc = match input.secret.as_deref().filter(|x| !x.is_empty()) {
        Some(secret) => Some(s.credentials.seal(&secret_aad(&id), secret.as_bytes()).map_err(ApiError::internal)?),
        None => None,
    };
    let target = StoredTarget {
        id,
        name: name.to_string(),
        kind: input.kind,
        endpoint: input.endpoint.trim().to_string(),
        location: input.location.trim().to_string(),
        username: input.username.trim().to_string(),
        secret_enc,
        auto_upload: input.auto_upload,
        problem: None,
    };
    repo::insert_target(&s.db, &target).await?;
    tracing::info!(target = %target.id, kind = ?target.kind, "export destination added");
    Ok(Json(target.public()))
}

/// The saved settings for the edit form (never the secret).
async fn settings(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<ExportTargetSettings>> {
    let target = repo::target_by_id(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Export destination"))?;
    Ok(Json(target.settings()))
}

/// The secret an edit uses: the one typed, else the saved one, but only for
/// the server it was saved for: a changed endpoint never receives it.
fn edit_secret(s: &AppState, saved: &StoredTarget, input: &ExportTargetInput) -> ApiResult<String> {
    if let Some(typed) = input.secret.as_deref().filter(|x| !x.is_empty()) {
        return Ok(typed.to_string());
    }
    let Some(enc) = &saved.secret_enc else { return Ok(String::new()) };
    if input.endpoint.trim() != saved.endpoint {
        return Err(ApiError::invalid("Enter the secret again: a saved secret is only sent to the server it was saved for"));
    }
    let secret = s.credentials.open(&secret_aad(&saved.id), enc).map_err(ApiError::internal)?;
    String::from_utf8(secret).map_err(ApiError::internal)
}

/// A saved destination, checked before its edit is applied.
async fn edited(s: &AppState, id: &str, input: &ExportTargetInput) -> ApiResult<(StoredTarget, String, ConnectionProbe)> {
    let saved = repo::target_by_id(&s.db, id).await?.ok_or_else(|| ApiError::not_found("Export destination"))?;
    if input.kind != saved.kind {
        return Err(ApiError::invalid("The service of a destination can't change: add a new destination instead"));
    }
    let secret = edit_secret(s, &saved, input)?;
    let p = probe(Provider::new(&TargetConfig { secret: &secret, ..config(input) })).await;
    Ok((saved, secret, p))
}

/// Try an edit before saving it, with the saved secret if none was typed.
async fn test_saved(State(s): State<AppState>, Path(id): Path<String>, Json(input): Json<ExportTargetInput>) -> ApiResult<Json<ConnectionProbe>> {
    edited(&s, &id, &input).await.map(|(_, _, p)| Json(p))
}

/// Save an edit; like adding, it must pass the test first.
async fn update(State(s): State<AppState>, Path(id): Path<String>, Json(input): Json<ExportTargetInput>) -> ApiResult<Json<ExportTarget>> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::invalid("Give the destination a name"));
    }
    let (saved, secret, p) = edited(&s, &id, &input).await?;
    if !p.ok {
        return Err(ApiError::invalid(p.message));
    }
    let secret_enc = match secret.is_empty() {
        true => None,
        false => Some(s.credentials.seal(&secret_aad(&id), secret.as_bytes()).map_err(ApiError::internal)?),
    };
    let target = StoredTarget {
        name: name.to_string(),
        endpoint: input.endpoint.trim().to_string(),
        location: input.location.trim().to_string(),
        username: input.username.trim().to_string(),
        secret_enc,
        auto_upload: input.auto_upload,
        problem: None,
        ..saved
    };
    if !repo::update_target(&s.db, &target).await? {
        return Err(ApiError::not_found("Export destination"));
    }
    tracing::info!(target = %target.id, "export destination edited");
    Ok(Json(target.public()))
}

#[derive(Deserialize)]
struct Rule {
    rule: AutoUpload,
}

async fn set_auto(State(s): State<AppState>, Path(id): Path<String>, Json(body): Json<Rule>) -> ApiResult<StatusCode> {
    if !repo::set_auto(&s.db, &id, body.rule).await? {
        return Err(ApiError::not_found("Export destination"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Is the destination reachable and writable right now? The same test as
/// when adding it; nothing is stored (only uploads and Reconnect record a
/// problem), so a short outage never blocks automatic uploads.
async fn check(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<ConnectionProbe>> {
    let target = repo::target_by_id(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Export destination"))?;
    Ok(Json(probe(s.exports.provider(&target)).await))
}

/// Test a saved destination again; clears or records its problem.
async fn reconnect(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<ExportTarget>> {
    let mut target = repo::target_by_id(&s.db, &id).await?.ok_or_else(|| ApiError::not_found("Export destination"))?;
    let p = probe(s.exports.provider(&target)).await;
    target.problem = (!p.ok).then_some(p.message);
    repo::set_problem(&s.db, &id, target.problem.as_deref()).await?;
    Ok(Json(target.public()))
}

async fn remove(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if !repo::delete_target(&s.db, &id).await? {
        return Err(ApiError::not_found("Export destination"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Uploads not finished yet (queued, waiting for a retry, running).
async fn active_jobs(State(s): State<AppState>) -> ApiResult<Json<Vec<ExportJob>>> {
    Ok(Json(repo::active_jobs(&s.db).await?))
}

async fn cancel_job(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<ExportJob>> {
    s.exports.cancel(&id).await.map(Json)
}

async fn job(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<ExportJob>> {
    repo::job_by_id(&s.db, &id).await?.map(Json).ok_or_else(|| ApiError::not_found("Export"))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    target_id: String,
}

/// `POST /api/v1/recordings/{id}/export`
pub async fn export_recording(State(s): State<AppState>, Path(recording_id): Path<String>, Json(body): Json<ExportRequest>) -> ApiResult<Json<ExportJob>> {
    s.exports.enqueue_recording(&recording_id, &body.target_id).await.map(Json)
}

/// `POST /api/v1/events/{id}/export`
pub async fn export_event(State(s): State<AppState>, Path(event_id): Path<String>, Json(body): Json<ExportRequest>) -> ApiResult<Json<ExportJob>> {
    s.exports.enqueue(&event_id, &body.target_id).await.map(Json)
}
