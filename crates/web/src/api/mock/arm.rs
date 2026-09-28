//! Mock armed surveillance: the same rules as the server (arming sets our
//! motion detection and recording on motion; disarm puts back what arming
//! changed and nobody touched since), kept in memory.

use std::cell::RefCell;

use chrono::Utc;

use super::db::with_db;
use super::sim::latency;
use crate::api::{ApiError, ApiResult, ArmInput, ArmSettings, ArmState, Id, Timestamp};
use crate::api::query::{Topic, invalidate};

thread_local! {
    /// Armed cameras: id, when, settings before.
    static ARMED: RefCell<Vec<(Id, Timestamp, ArmSettings)>> = const { RefCell::new(Vec::new()) };
}

fn current() -> ArmState {
    ARMED.with_borrow(|a| ArmState { armed: !a.is_empty(), since: a.iter().map(|(_, t, _)| *t).min(), cameras: a.iter().map(|(id, _, _)| id.clone()).collect() })
}

fn settings_of(id: &str) -> Option<ArmSettings> {
    with_db(|db| db.cameras.iter().find(|c| c.id == id).map(|c| ArmSettings { motion_enabled: c.motion.enabled, source: c.motion.source, mode: c.recording.mode }))
}

fn apply(id: &str, s: ArmSettings) {
    with_db(|db| {
        if let Some(c) = db.cameras.iter_mut().find(|c| c.id == id) {
            c.motion.enabled = s.motion_enabled;
            c.motion.source = s.source;
            c.recording.mode = s.mode;
        }
    });
}

pub async fn state() -> ApiResult<ArmState> {
    latency().await;
    Ok(current())
}

pub async fn change(input: ArmInput) -> ApiResult<ArmState> {
    latency().await;
    let all: Vec<Id> = with_db(|db| db.cameras.iter().map(|c| c.id.clone()).collect());
    if input.armed {
        if let Some(unknown) = input.cameras.iter().find(|id| !all.contains(id)) {
            return Err(ApiError::new(400, "invalid", format!("There is no camera {unknown}")));
        }
        let chosen = if input.cameras.is_empty() { all } else { input.cameras };
        for id in chosen {
            if ARMED.with_borrow(|a| a.iter().any(|(a, _, _)| *a == id)) {
                continue;
            }
            let Some(before) = settings_of(&id) else { continue };
            ARMED.with_borrow_mut(|a| a.push((id.clone(), Utc::now(), before)));
            apply(&id, before.armed());
        }
    } else {
        for (id, _, before) in ARMED.with_borrow_mut(std::mem::take) {
            if let Some(now) = settings_of(&id) {
                apply(&id, now.disarmed(before));
            }
        }
    }
    invalidate(Topic::Cameras);
    Ok(current())
}
