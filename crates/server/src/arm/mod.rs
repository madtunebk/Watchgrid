//! Armed surveillance (Dashboard): arm the chosen cameras (Watchgrid's own
//! motion detection, recording on motion) while nobody is home; disarm
//! puts each camera back as it was. What arming changed is kept per camera
//! in `armed_cameras`, so a restart in between loses nothing.

mod repo;
mod routes;

use watchgrid_model::ArmState;

pub use routes::router;

use crate::cameras;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub async fn state(db: &sqlx::PgPool) -> sqlx::Result<ArmState> {
    let rows = repo::list(db).await?;
    Ok(ArmState { armed: !rows.is_empty(), since: rows.iter().map(|r| r.armed_at).min(), cameras: rows.into_iter().map(|r| r.camera_id).collect() })
}

/// Arm `chosen` (empty = every camera); cameras already armed stay as they are.
pub async fn arm(state: &AppState, chosen: &[String]) -> ApiResult<ArmState> {
    let all: Vec<String> = cameras::all_ids(state).await?.into_iter().map(|(id, _)| id).collect();
    if let Some(unknown) = chosen.iter().find(|id| !all.contains(id)) {
        return Err(ApiError::invalid(format!("There is no camera {unknown}")));
    }
    let chosen = if chosen.is_empty() { all } else { chosen.to_vec() };
    let armed = state_ids(state).await?;
    for id in chosen.iter().filter(|id| !armed.contains(id)) {
        let Some(before) = cameras::arm_settings(&state.db, id).await? else { continue };
        // Kept first: if anything stops half-way, disarm still knows.
        repo::insert(&state.db, id, before).await?;
        cameras::set_arm_settings(state, id, before.armed()).await?;
        tracing::info!(camera = %id, "armed");
    }
    Ok(self::state(&state.db).await?)
}

/// Disarm every armed camera: back to its settings from before arming,
/// except what was changed by hand meanwhile.
pub async fn disarm(state: &AppState) -> ApiResult<ArmState> {
    for row in repo::list(&state.db).await? {
        if let Some(now) = cameras::arm_settings(&state.db, &row.camera_id).await? {
            let back = now.disarmed(row.before.0);
            if back != now {
                cameras::set_arm_settings(state, &row.camera_id, back).await?;
            }
        }
        repo::delete(&state.db, &row.camera_id).await?;
        tracing::info!(camera = %row.camera_id, "disarmed");
    }
    Ok(self::state(&state.db).await?)
}

async fn state_ids(state: &AppState) -> sqlx::Result<Vec<String>> {
    Ok(repo::list(&state.db).await?.into_iter().map(|r| r.camera_id).collect())
}

#[cfg(test)]
mod tests;
