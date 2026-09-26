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

pub async fn state(id: &str) -> ApiResult<PtzState> {
    latency().await;
    Ok(if id == MOVING_CAMERA { PtzState { available: true, presets: PRESETS.with_borrow(Clone::clone) } } else { PtzState::default() })
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
