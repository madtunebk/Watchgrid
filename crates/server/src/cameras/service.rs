//! Camera management rules: ids, secrets, uniqueness. Changes apply
//! immediately; later the RTSP supervisor will be notified from here.

use watchgrid_model::{Camera, CameraInput, CameraStatus};

use std::collections::HashMap;

use sqlx::PgPool;

use super::repo::{self, Secret};
use super::{url_credentials, validate};
use crate::bus::BusEvent;
use crate::recordings;
use crate::credentials::CredentialStore;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

fn aad(id: &str, which: &str) -> String {
    format!("camera:{id}:{which}")
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "camera".into() } else { s }
}

async fn new_id(state: &AppState, name: &str) -> ApiResult<String> {
    let base = format!("cam-{}", slug(name));
    let mut id = base.clone();
    let mut n = 2;
    while repo::id_taken(&state.db, &id).await? {
        id = format!("{base}-{n}");
        n += 1;
    }
    Ok(id)
}

fn seal(state: &AppState, id: &str, which: &str, secret: &str) -> ApiResult<Vec<u8>> {
    state.credentials.seal(&aad(id, which), secret.as_bytes()).map_err(ApiError::internal)
}

fn duplicate(name: &str) -> ApiError {
    ApiError::conflict(format!("A camera named \"{}\" already exists", name.trim()))
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|s| !s.is_empty())
}

/// Move any `user:password@` out of the stream URLs into the camera's own
/// username/password, so URLs are stored and returned without secrets.
/// An explicitly entered password wins over one found in a URL.
fn lift_url_credentials(input: &mut CameraInput) {
    let mut found = Vec::new();
    let main = url_credentials::split(&input.main_stream_url);
    input.main_stream_url = main.url.clone();
    found.push(main);
    if let Some(sub) = input.sub_stream_url.as_mut() {
        let s = url_credentials::split(sub);
        *sub = s.url.clone();
        found.push(s);
    }
    for f in found {
        if input.username.trim().is_empty()
            && let Some(u) = f.username
        {
            input.username = u;
        }
        if non_empty(&input.password).is_none()
            && let Some(p) = f.password
        {
            input.password = Some(p);
        }
    }
}

/// Stored config with the live state (status, stream details) on top.
fn with_live(state: &AppState, row: repo::CameraRow, usage: &HashMap<String, u64>) -> Camera {
    let mut camera = row.into_model();
    camera.storage_used = Some(usage.get(&camera.id).copied().unwrap_or(0));
    state.live.overlay(&mut camera);
    state.recorder.overlay(&mut camera);
    camera
}

/// Start a manual recording on the main stream.
pub async fn start_recording(state: &AppState, id: &str) -> ApiResult<Camera> {
    let camera = get(state, id).await?;
    if !camera.enabled {
        return Err(ApiError::conflict("The camera is disabled"));
    }
    if camera.status != CameraStatus::Online {
        return Err(ApiError::conflict("The camera is not online"));
    }
    state.recorder.start(id);
    tracing::info!(camera = %id, "manual recording requested");
    get(state, id).await
}

/// Stop a manual recording and wait for the file to be finalized.
pub async fn stop_recording(state: &AppState, id: &str) -> ApiResult<Camera> {
    get(state, id).await?;
    tracing::info!(camera = %id, "manual recording stop requested");
    state.recorder.stop(id).await;
    get(state, id).await
}

pub async fn list(state: &AppState) -> ApiResult<Vec<Camera>> {
    let usage = recording_bytes(state).await?;
    Ok(repo::list(&state.db).await?.into_iter().map(|r| with_live(state, r, &usage)).collect())
}

pub async fn get(state: &AppState, id: &str) -> ApiResult<Camera> {
    let row = repo::get(&state.db, id).await?.ok_or_else(|| ApiError::not_found("Camera"))?;
    Ok(with_live(state, row, &recording_bytes(state).await?))
}

/// Bytes of recordings per camera id.
async fn recording_bytes(state: &AppState) -> ApiResult<HashMap<String, u64>> {
    Ok(recordings::usage_by_camera(&state.db).await?.into_iter().map(|u| (u.camera_id, u.bytes)).collect())
}

/// Ids and enabled flags of all cameras (supervisor start-up).
pub async fn all_ids(state: &AppState) -> ApiResult<Vec<(String, bool)>> {
    Ok(repo::list(&state.db).await?.into_iter().map(|r| r.into_model()).map(|c| (c.id, c.enabled)).collect())
}

pub async fn create(state: &AppState, mut input: CameraInput) -> ApiResult<Camera> {
    lift_url_credentials(&mut input);
    validate::check(&input)?;
    let id = new_id(state, &input.name).await?;
    let password = non_empty(&input.password).map(|p| seal(state, &id, "password", p)).transpose()?;
    let onvif_password = input.onvif.as_ref().and_then(|o| non_empty(&o.password)).map(|p| seal(state, &id, "onvif-password", p)).transpose()?;
    match repo::insert(&state.db, &id, &input, password, onvif_password).await {
        Err(e) if repo::is_unique_violation(&e) => return Err(duplicate(&input.name)),
        other => other?,
    }
    tracing::info!(camera = %id, "camera added");
    changed(state, &id, Some(input.enabled));
    get(state, &id).await
}

pub async fn update(state: &AppState, id: &str, mut input: CameraInput) -> ApiResult<Camera> {
    lift_url_credentials(&mut input);
    validate::check(&input)?;
    // An empty/absent password means "keep the stored one".
    let password = match non_empty(&input.password) {
        Some(p) => Secret::Set(seal(state, id, "password", p)?),
        None => Secret::Keep,
    };
    let onvif_password = match &input.onvif {
        None => Secret::Clear,
        Some(o) => match non_empty(&o.password) {
            Some(p) => Secret::Set(seal(state, id, "onvif-password", p)?),
            None => Secret::Keep,
        },
    };
    match repo::update(&state.db, id, &input, password, onvif_password).await {
        Err(e) if repo::is_unique_violation(&e) => Err(duplicate(&input.name)),
        Err(e) => Err(e.into()),
        Ok(false) => Err(ApiError::not_found("Camera")),
        Ok(true) => {
            tracing::info!(camera = %id, "camera updated");
            changed(state, id, Some(input.enabled));
            get(state, id).await
        }
    }
}

pub async fn set_enabled(state: &AppState, id: &str, enabled: bool) -> ApiResult<Camera> {
    if !repo::set_enabled(&state.db, id, enabled).await? {
        return Err(ApiError::not_found("Camera"));
    }
    tracing::info!(camera = %id, enabled, "camera availability changed");
    changed(state, id, Some(enabled));
    get(state, id).await
}

pub async fn delete(state: &AppState, id: &str) -> ApiResult<()> {
    if !repo::delete(&state.db, id).await? {
        return Err(ApiError::not_found("Camera"));
    }
    tracing::info!(camera = %id, "camera deleted");
    changed(state, id, None);
    Ok(())
}

/// What the supervisor needs to connect. Server-internal only: never
/// returned by the HTTP API. `None` when the camera does not exist.
pub struct ConnectionInfo {
    pub enabled: bool,
    pub main_url: String,
    pub sub_url: Option<String>,
    pub username: String,
    pub password: Option<String>,
}

pub async fn connection_info(db: &PgPool, credentials: &CredentialStore, id: &str) -> Result<Option<ConnectionInfo>, String> {
    let Some(row) = repo::get(db, id).await.map_err(|e| e.to_string())? else { return Ok(None) };
    let camera = row.into_model();
    let password = match repo::password_enc(db, id).await.map_err(|e| e.to_string())?.flatten() {
        None => None,
        Some(bytes) => {
            let plain = credentials.open(&aad(id, "password"), &bytes).map_err(|e| e.to_string())?;
            Some(String::from_utf8(plain).map_err(|e| e.to_string())?)
        }
    };
    Ok(Some(ConnectionInfo {
        enabled: camera.enabled,
        main_url: camera.main_stream.url,
        sub_url: camera.sub_stream.map(|s| s.url).filter(|u| !u.is_empty()),
        username: camera.username,
        password,
    }))
}

/// Stored ONVIF username and password of the camera configured with this
/// ONVIF URL (so "Test ONVIF" works while editing without retyping it).
pub async fn stored_onvif_login(state: &AppState, url: &str) -> ApiResult<Option<(String, Option<String>)>> {
    let Some((id, config, enc)) = repo::onvif_by_url(&state.db, url).await? else { return Ok(None) };
    let password = match enc {
        None => None,
        Some(bytes) => {
            let plain = state.credentials.open(&aad(&id, "onvif-password"), &bytes).map_err(ApiError::internal)?;
            Some(String::from_utf8(plain).map_err(ApiError::internal)?)
        }
    };
    Ok(Some((config.0.username, password)))
}

/// What the ONVIF event watcher needs. `None` when the camera is gone.
pub struct OnvifWatch {
    /// Enabled, with ONVIF settings and motion taken from ONVIF events.
    pub active: bool,
    pub url: String,
    pub username: String,
    pub password: Option<String>,
}

pub async fn onvif_watch(db: &PgPool, credentials: &CredentialStore, id: &str) -> Result<Option<OnvifWatch>, String> {
    let Some(row) = repo::get(db, id).await.map_err(|e| e.to_string())? else { return Ok(None) };
    let camera = row.into_model();
    let Some(onvif) = camera.onvif.filter(|o| !o.url.trim().is_empty()) else {
        return Ok(Some(OnvifWatch { active: false, url: String::new(), username: String::new(), password: None }));
    };
    let active = camera.enabled && camera.motion.enabled && camera.motion.source == watchgrid_model::MotionSource::Onvif;
    let password = match repo::onvif_password_enc(db, id).await.map_err(|e| e.to_string())?.flatten() {
        None => None,
        Some(bytes) => Some(String::from_utf8(credentials.open(&aad(id, "onvif-password"), &bytes).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?),
    };
    Ok(Some(OnvifWatch { active, url: onvif.url, username: onvif.username, password }))
}

/// Username and decrypted password (for `watchgrid probe`).
pub async fn stream_credentials(state: &AppState, id: &str) -> ApiResult<(String, Option<String>)> {
    let info = connection_info(&state.db, &state.credentials, id).await.map_err(ApiError::internal)?.ok_or_else(|| ApiError::not_found("Camera"))?;
    Ok((info.username, info.password))
}

/// Tell the supervisor and UI subscribers that a camera's config changed.
fn changed(state: &AppState, id: &str, enabled: Option<bool>) {
    match enabled {
        Some(enabled) => state.supervisor.apply(id, enabled),
        None => state.supervisor.stop(id),
    }
    state.onvif.apply(id, enabled.is_some());
    if enabled != Some(true) {
        state.bus.publish(BusEvent::CameraStopped { camera_id: id.to_string(), at: chrono::Utc::now() });
    }
    state.media.reload(id);
    state.bus.publish(BusEvent::CamerasChanged);
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slugs_are_url_safe() {
        assert_eq!(slug("Front Door"), "front-door");
        assert_eq!(slug("  Garage #2 (east) "), "garage-2-east");
        assert_eq!(slug("!!!"), "camera");
    }
}
