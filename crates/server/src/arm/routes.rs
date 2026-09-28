//! `/api/v1/arm`: GET the state, POST `{ armed, cameras }` to arm or disarm
//! (a write, so administrators only).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use watchgrid_model::{ArmInput, ArmState};

use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(show).post(change))
}

async fn show(State(s): State<AppState>) -> ApiResult<Json<ArmState>> {
    Ok(Json(super::state(&s.db).await?))
}

async fn change(State(s): State<AppState>, Json(input): Json<ArmInput>) -> ApiResult<Json<ArmState>> {
    let state = if input.armed { super::arm(&s, &input.cameras).await? } else { super::disarm(&s).await? };
    Ok(Json(state))
}
