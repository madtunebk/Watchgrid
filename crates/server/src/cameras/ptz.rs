//! `/api/v1/cameras/{id}/ptz/*` — move cameras that can, and their presets.
//!
//! Opening the camera's PTZ service costs three ONVIF requests, while a
//! held arrow sends a move twice a second: the session is kept per camera
//! and dropped when the camera changes or a request fails.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use watchgrid_model::{PtzMove, PtzPreset, PtzPresetInput, PtzState};

use super::service;
use crate::error::{ApiError, ApiResult};
use crate::onvif::ptz::Ptz;
use crate::state::AppState;

/// PTZ sessions by camera; `None` = the camera has no PTZ.
#[derive(Default)]
pub struct PtzSessions(Mutex<HashMap<String, Option<Arc<Ptz>>>>);

impl PtzSessions {
    /// The camera changed (or a request failed): reconnect next time.
    pub fn forget(&self, camera_id: &str) {
        self.0.lock().expect("ptz lock").remove(camera_id);
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}/ptz", get(state))
        .route("/{id}/ptz/move", post(move_camera))
        .route("/{id}/ptz/stop", post(stop))
        .route("/{id}/ptz/presets", post(save_preset))
        .route("/{id}/ptz/presets/{token}/goto", post(goto_preset))
        .route("/{id}/ptz/presets/{token}", delete(remove_preset))
}

async fn session(s: &AppState, id: &str) -> ApiResult<Option<Arc<Ptz>>> {
    if let Some(cached) = s.ptz.0.lock().expect("ptz lock").get(id) {
        return Ok(cached.clone());
    }
    let camera = service::get(s, id).await?;
    let Some(onvif) = camera.onvif.filter(|o| !o.url.trim().is_empty()) else { return Ok(None) };
    let (user, password) = service::stored_onvif_login(s, &onvif.url).await?.unwrap_or_default();
    let ptz = Ptz::connect(&onvif.url, &user, password)
        .await
        .map_err(|e| {
            tracing::warn!(camera = %id, "PTZ: cannot reach the camera: {e}");
            ApiError::conflict(format!("Cannot reach the camera's PTZ: {e}"))
        })?
        .map(Arc::new);
    s.ptz.0.lock().expect("ptz lock").insert(id.to_string(), ptz.clone());
    Ok(ptz)
}

/// The session, or 409 for cameras that don't move.
async fn required(s: &AppState, id: &str) -> ApiResult<Arc<Ptz>> {
    session(s, id).await?.ok_or_else(|| ApiError::conflict("This camera has no pan / tilt"))
}

/// A camera error drops the session so the next request reconnects.
fn failed(s: &AppState, id: &str, e: String) -> ApiError {
    s.ptz.forget(id);
    ApiError::conflict(format!("The camera refused: {e}"))
}

/// Whether the camera moves, and its presets. A preset list the camera
/// refuses doesn't take the arrows away: it comes back as `presetsError`.
async fn state(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<PtzState>> {
    let Some(ptz) = session(&s, &id).await? else { return Ok(Json(PtzState::default())) };
    let (presets, presets_error) = match ptz.presets().await {
        Ok(list) => (list.into_iter().map(|p| PtzPreset { token: p.token, name: p.name }).collect(), None),
        Err(e) => {
            tracing::warn!(camera = %id, "PTZ: the camera didn't list its presets: {e}");
            s.ptz.forget(&id);
            (Vec::new(), Some(format!("The camera didn't list its saved positions: {e}")))
        }
    };
    Ok(Json(PtzState { available: true, presets, presets_error }))
}

async fn move_camera(State(s): State<AppState>, Path(id): Path<String>, Json(m): Json<PtzMove>) -> ApiResult<StatusCode> {
    let ptz = required(&s, &id).await?;
    ptz.move_at(m.pan, m.tilt, m.zoom).await.map_err(|e| failed(&s, &id, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let ptz = required(&s, &id).await?;
    ptz.stop().await.map_err(|e| failed(&s, &id, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn save_preset(State(s): State<AppState>, Path(id): Path<String>, Json(input): Json<PtzPresetInput>) -> ApiResult<Json<PtzPreset>> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(ApiError::invalid("Give the preset a name of at most 40 characters"));
    }
    let ptz = required(&s, &id).await?;
    let p = ptz.save_preset(name).await.map_err(|e| failed(&s, &id, e))?;
    Ok(Json(PtzPreset { token: p.token, name: p.name }))
}

async fn goto_preset(State(s): State<AppState>, Path((id, token)): Path<(String, String)>) -> ApiResult<StatusCode> {
    let ptz = required(&s, &id).await?;
    ptz.goto_preset(&token).await.map_err(|e| failed(&s, &id, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_preset(State(s): State<AppState>, Path((id, token)): Path<(String, String)>) -> ApiResult<StatusCode> {
    let ptz = required(&s, &id).await?;
    ptz.remove_preset(&token).await.map_err(|e| failed(&s, &id, e))?;
    Ok(StatusCode::NO_CONTENT)
}
