//! Mock PTZ: the Driveway camera "moves" and keeps presets in memory.

use std::cell::RefCell;

use super::sim::latency;
use crate::api::{ApiError, ApiResult, PtzMove, PtzPreset, PtzState};

const MOVING_CAMERA: &str = "cam-driveway";

thread_local! {
    static PRESETS: RefCell<Vec<PtzPreset>> = RefCell::new(vec![
        PtzPreset { token: "1".into(), name: "Gate".into() },
        PtzPreset { token: "2".into(), name: "Street".into() },
    ]);
}

fn check(id: &str) -> ApiResult<()> {
    if id == MOVING_CAMERA { Ok(()) } else { Err(ApiError::conflict("This camera has no pan / tilt")) }
}

/// `?ptz=presets`: the camera moves but won't list its presets;
/// `?ptz=unreachable`: its PTZ can't be reached at all.
pub async fn state(id: &str) -> ApiResult<PtzState> {
    latency().await;
    if id != MOVING_CAMERA {
        return Ok(PtzState::default());
    }
    match super::scenario::param("ptz").as_str() {
        "unreachable" => Err(ApiError::conflict("Cannot reach the camera's PTZ: GetCapabilities: the camera closed the connection without answering")),
        "presets" => Ok(PtzState {
            available: true,
            presets: Vec::new(),
            presets_error: Some("The camera didn't list its saved positions: GetPresets: the camera answered HTTP 500".into()),
        }),
        _ => Ok(PtzState { available: true, presets: PRESETS.with_borrow(Clone::clone), presets_error: None }),
    }
}

pub async fn move_at(id: &str, _m: PtzMove) -> ApiResult<()> {
    check(id)
}

pub async fn stop(id: &str) -> ApiResult<()> {
    check(id)
}

pub async fn goto(id: &str, _token: &str) -> ApiResult<()> {
    latency().await;
    check(id)
}

pub async fn save(id: &str, name: &str) -> ApiResult<PtzPreset> {
    latency().await;
    check(id)?;
    Ok(PRESETS.with_borrow_mut(|list| {
        let preset = PtzPreset { token: (list.len() + 1).to_string(), name: name.to_string() };
        list.push(preset.clone());
        preset
    }))
}

pub async fn remove(id: &str, token: &str) -> ApiResult<()> {
    latency().await;
    check(id)?;
    PRESETS.with_borrow_mut(|list| list.retain(|p| p.token != token));
    Ok(())
}
