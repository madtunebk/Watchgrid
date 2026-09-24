//! `/api/v1/settings` endpoints.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use watchgrid_model::Settings;

use super::app;
use crate::bus::BusEvent;
use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(read).put(update))
}

async fn read(State(s): State<AppState>) -> ApiResult<Json<Settings>> {
    app::load(&s.db, s.bind).await.map(Json)
}

async fn update(State(s): State<AppState>, Json(input): Json<Settings>) -> ApiResult<Json<Settings>> {
    let saved = app::save(&s.db, input).await?;
    s.bus.publish(BusEvent::SettingsChanged);
    Ok(Json(saved))
}
